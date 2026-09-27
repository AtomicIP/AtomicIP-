#!/usr/bin/env bash
# #1066: Automated backup verification.
#
# Proves that the latest backups can actually be restored, not merely that they
# exist. Run on a schedule (deploy/k8s/jobs/backup-verification-cronjob.yaml and
# .github/workflows/backup-verification.yml). Steps:
#
#   1. Freshness    latest Postgres base backup / WAL / audit archive are within RPO
#   2. Integrity    SHA256SUMS + pg_verifybackup against the backup manifest
#   3. Restore      restore base backup + replay WAL into a throwaway Postgres
#   4. Validation   schema present, row counts sane, amcheck index verification
#   5. RTO          time the restore and compare with the DR target
#   6. Report       push metrics; on any failure fire an alert to Alertmanager
#
# Environment:
#   BACKUP_BUCKET          s3://bucket/prefix holding postgres/base, postgres/wal, audit/
#   PG_IMAGE               Postgres image matching production (default postgres:16)
#   RTO_TARGET_SECONDS     DB restore target (default 7200, docs/disaster-recovery.md §2)
#   BASE_BACKUP_MAX_AGE    max age of newest base backup in seconds (default 93600 = 26h)
#   WAL_MAX_AGE            max age of newest WAL segment in seconds (default 300 = RPO 5 min)
#   AUDIT_MAX_AGE          max age of newest audit archive in seconds (default 1800)
#   EXPECTED_TABLES        space-separated tables that must exist (default below)
#   PUSHGATEWAY_URL        optional Prometheus Pushgateway
#   ALERTMANAGER_URL       optional Alertmanager (direct alert on failure)
#   KEEP_WORKDIR=1         keep the restored data directory for debugging
set -euo pipefail

: "${BACKUP_BUCKET:?BACKUP_BUCKET is required}"
PG_IMAGE="${PG_IMAGE:-postgres:16}"
RTO_TARGET_SECONDS="${RTO_TARGET_SECONDS:-7200}"
BASE_BACKUP_MAX_AGE="${BASE_BACKUP_MAX_AGE:-93600}"
WAL_MAX_AGE="${WAL_MAX_AGE:-300}"
AUDIT_MAX_AGE="${AUDIT_MAX_AGE:-1800}"
EXPECTED_TABLES="${EXPECTED_TABLES:-sessions webhooks webhook_deliveries ip_index audit_events}"
BUCKET="${BACKUP_BUCKET%/}"

WORKDIR=$(mktemp -d -t atomicip-backup-verify-XXXXXX)
CONTAINER="atomicip-restore-$$"
STARTED=$(date +%s)
FAILURES=()
declare -A METRICS

log()  { printf '[backup-verify] %s\n' "$*" >&2; }
fail() { log "FAIL: $*"; FAILURES+=("$*"); }

cleanup() {
  docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
  [ "${KEEP_WORKDIR:-0}" = "1" ] && log "workdir kept at $WORKDIR" || rm -rf "$WORKDIR"
}
trap cleanup EXIT

newest_object() { # prefix -> "epoch key"
  aws s3 ls "$1" --recursive | sort -k1,2 | tail -n1 \
    | awk '{ cmd = "date -u -d \"" $1 " " $2 "\" +%s"; cmd | getline t; close(cmd); print t, $4 }'
}

check_age() { # label prefix max_age
  local newest ts key age
  newest=$(newest_object "$2")
  if [ -z "$newest" ]; then fail "$1: no backups found under $2"; return; fi
  read -r ts key <<< "$newest"
  age=$(( $(date +%s) - ts ))
  METRICS["atomicip_backup_age_seconds{kind=\"$1\"}"]=$age
  if [ "$age" -gt "$3" ]; then
    fail "$1: newest backup $key is ${age}s old (limit $3s)"
  else
    log "$1: newest backup $key is ${age}s old"
  fi
}

# 1. Freshness --------------------------------------------------------------
check_age postgres_base "$BUCKET/postgres/base/" "$BASE_BACKUP_MAX_AGE"
check_age postgres_wal  "$BUCKET/postgres/wal/"  "$WAL_MAX_AGE"
check_age audit_log     "$BUCKET/audit/"         "$AUDIT_MAX_AGE"

# 2. Integrity --------------------------------------------------------------
LATEST_BASE=$(aws s3 ls "$BUCKET/postgres/base/" | awk '/PRE/ {print $2}' | sort | tail -n1 | tr -d /)
if [ -z "$LATEST_BASE" ]; then
  fail "no base backup directory found"
else
  log "verifying base backup $LATEST_BASE"
  DL_START=$(date +%s)
  aws s3 cp --recursive --only-show-errors "$BUCKET/postgres/base/$LATEST_BASE/" "$WORKDIR/backup/"
  METRICS[atomicip_backup_download_seconds]=$(( $(date +%s) - DL_START ))

  if [ -f "$WORKDIR/backup/SHA256SUMS" ]; then
    (cd "$WORKDIR/backup" && sha256sum --quiet -c SHA256SUMS) \
      || fail "checksum mismatch in $LATEST_BASE"
  else
    fail "$LATEST_BASE has no SHA256SUMS"
  fi

  mkdir -p "$WORKDIR/pgdata"
  tar -xzf "$WORKDIR/backup/base.tar.gz" -C "$WORKDIR/pgdata"
  [ -f "$WORKDIR/backup/pg_wal.tar.gz" ] && mkdir -p "$WORKDIR/pgdata/pg_wal" \
    && tar -xzf "$WORKDIR/backup/pg_wal.tar.gz" -C "$WORKDIR/pgdata/pg_wal"
  cp "$WORKDIR/backup/backup_manifest" "$WORKDIR/pgdata/" 2>/dev/null \
    || fail "$LATEST_BASE has no backup_manifest"

  docker run --rm -v "$WORKDIR/pgdata:/pgdata" "$PG_IMAGE" \
    pg_verifybackup --no-parse-wal /pgdata >/dev/null \
    || fail "pg_verifybackup failed for $LATEST_BASE"
fi

# 3. Restore + WAL replay ---------------------------------------------------
if [ -d "$WORKDIR/pgdata" ] && [ ${#FAILURES[@]} -eq 0 ]; then
  RESTORE_START=$(date +%s)
  mkdir -p "$WORKDIR/wal"
  aws s3 sync --only-show-errors "$BUCKET/postgres/wal/" "$WORKDIR/wal/"
  touch "$WORKDIR/pgdata/recovery.signal"
  cat >> "$WORKDIR/pgdata/postgresql.auto.conf" <<CONF
restore_command = 'cp /wal/%f %p'
recovery_target_timeline = 'latest'
recovery_target_action = 'promote'
archive_mode = off
CONF
  sudo_chown() { docker run --rm -v "$WORKDIR/pgdata:/pgdata" "$PG_IMAGE" chown -R postgres:postgres /pgdata; }
  sudo_chown

  docker run -d --name "$CONTAINER" \
    -v "$WORKDIR/pgdata:/var/lib/postgresql/data" -v "$WORKDIR/wal:/wal:ro" \
    -e POSTGRES_HOST_AUTH_METHOD=trust "$PG_IMAGE" >/dev/null

  deadline=$(( $(date +%s) + RTO_TARGET_SECONDS ))
  until docker exec "$CONTAINER" psql -U postgres -Atc "SELECT NOT pg_is_in_recovery()" 2>/dev/null | grep -q t; do
    if [ "$(date +%s)" -gt "$deadline" ]; then fail "restore did not finish within RTO ${RTO_TARGET_SECONDS}s"; break; fi
    if ! docker ps -q -f name="$CONTAINER" | grep -q .; then
      docker logs "$CONTAINER" 2>&1 | tail -n 30 >&2 || true
      fail "restored Postgres exited during recovery"; break
    fi
    sleep 5
  done
  RESTORE_SECONDS=$(( $(date +%s) - RESTORE_START ))
  METRICS[atomicip_backup_restore_seconds]=$RESTORE_SECONDS
  log "restore + WAL replay took ${RESTORE_SECONDS}s (target ${RTO_TARGET_SECONDS}s)"

  # 4. Validation -----------------------------------------------------------
  if [ ${#FAILURES[@]} -eq 0 ]; then
    q() { docker exec "$CONTAINER" psql -U postgres -d "${PGDATABASE:-atomicip}" -v ON_ERROR_STOP=1 -Atc "$1"; }
    for t in $EXPECTED_TABLES; do
      if [ "$(q "SELECT to_regclass('public.$t') IS NOT NULL")" != "t" ]; then
        fail "table $t missing from restored database"
      else
        rows=$(q "SELECT count(*) FROM $t")
        METRICS["atomicip_backup_restored_rows{table=\"$t\"}"]=$rows
      fi
    done
    # Last committed transaction in the restore approximates achieved RPO.
    last_xact=$(q "SELECT COALESCE(extract(epoch FROM pg_last_committed_xact_timestamp())::bigint, 0)" 2>/dev/null || echo 0)
    [ "$last_xact" -gt 0 ] && METRICS[atomicip_backup_rpo_seconds]=$(( STARTED - last_xact ))

    # Structural integrity of every btree index (catches silent corruption).
    q "CREATE EXTENSION IF NOT EXISTS amcheck" >/dev/null
    bad=$(q "SELECT count(*) FROM (
               SELECT bt_index_check(c.oid) FROM pg_index i
                 JOIN pg_class c ON c.oid = i.indexrelid
                 JOIN pg_am a ON a.oid = c.relam
                 JOIN pg_namespace n ON n.oid = c.relnamespace
                WHERE a.amname = 'btree' AND n.nspname = 'public' AND c.relpersistence <> 't') s" 2>&1) \
      || fail "amcheck reported index corruption: $bad"
  fi
  # 5. RTO ------------------------------------------------------------------
  [ "$RESTORE_SECONDS" -le "$RTO_TARGET_SECONDS" ] || fail "restore time ${RESTORE_SECONDS}s exceeds RTO ${RTO_TARGET_SECONDS}s"
fi

# Audit log archive: newest object must decompress cleanly.
AUDIT_KEY=$(newest_object "$BUCKET/audit/" | awk '{print $2}')
if [ -n "$AUDIT_KEY" ]; then
  aws s3 cp --only-show-errors "s3://$(echo "${BUCKET#s3://}" | cut -d/ -f1)/$AUDIT_KEY" "$WORKDIR/audit-sample"
  case "$AUDIT_KEY" in
    *.zst) zstd -t -q "$WORKDIR/audit-sample" || fail "audit archive $AUDIT_KEY is corrupt" ;;
    *.gz)  gzip -t "$WORKDIR/audit-sample"    || fail "audit archive $AUDIT_KEY is corrupt" ;;
  esac
fi

# 6. Report -----------------------------------------------------------------
TOTAL=$(( $(date +%s) - STARTED ))
SUCCESS=$([ ${#FAILURES[@]} -eq 0 ] && echo 1 || echo 0)
METRICS[atomicip_backup_verify_success]=$SUCCESS
METRICS[atomicip_backup_verify_duration_seconds]=$TOTAL
METRICS[atomicip_backup_verify_last_run_timestamp_seconds]=$(date +%s)
[ "$SUCCESS" = 1 ] && METRICS[atomicip_backup_verify_last_success_timestamp_seconds]=$(date +%s)

if [ -n "${PUSHGATEWAY_URL:-}" ]; then
  for k in "${!METRICS[@]}"; do printf '%s %s\n' "$k" "${METRICS[$k]}"; done \
    | curl -fsS --data-binary @- "${PUSHGATEWAY_URL}/metrics/job/backup_verification" >/dev/null \
    || log "pushgateway unreachable"
fi

jq -n --arg base "${LATEST_BASE:-}" --argjson ok "$SUCCESS" --argjson total "$TOTAL" \
  --argjson restore "${RESTORE_SECONDS:-null}" --argjson rto "$RTO_TARGET_SECONDS" \
  --args '{base_backup:$base, success:($ok == 1), duration_seconds:$total,
           restore_seconds:$restore, rto_target_seconds:$rto, failures:$ARGS.positional}' \
  "${FAILURES[@]}" > "${REPORT_OUT:-backup-verification.json}"

if [ "$SUCCESS" = 1 ]; then
  log "PASS: backup $LATEST_BASE restored in ${RESTORE_SECONDS:-?}s"
  exit 0
fi

if [ -n "${ALERTMANAGER_URL:-}" ]; then
  jq -cn --arg summary "Backup verification failed: ${FAILURES[0]}" \
    --arg desc "$(printf '%s; ' "${FAILURES[@]}")" \
    '[{labels:{alertname:"BackupVerificationFailed", severity:"critical", service:"backups", team:"platform"},
       annotations:{summary:$summary, description:$desc, runbook:"docs/backup-strategy.md#5-when-verification-fails"}}]' \
    | curl -fsS -H 'Content-Type: application/json' --data-binary @- \
        "${ALERTMANAGER_URL%/}/api/v2/alerts" >/dev/null || log "alertmanager unreachable"
fi
log "verification failed with ${#FAILURES[@]} problem(s)"
exit 1
