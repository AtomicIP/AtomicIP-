#!/usr/bin/env bash
# #1067: Cost optimization automation.
#
#   scripts/ops/cost-optimize.sh cleanup   # delete orphaned / expired off-chain data
#   scripts/ops/cost-optimize.sh compress  # compress + tier old audit logs and exports
#   scripts/ops/cost-optimize.sh report    # summarise savings for the last N days
#   scripts/ops/cost-optimize.sh all       # cleanup, compress, report
#
# Every action appends a JSON line to $COST_LEDGER describing what it saved, so
# savings are tracked over time and can be reported on. Metrics are also pushed
# to a Prometheus Pushgateway when PUSHGATEWAY_URL is set.
#
# Only off-chain caches/indexes are touched: the Stellar ledger is the source of
# truth (docs/disaster-recovery.md §1) and nothing here can affect it.
#
# Environment:
#   DATABASE_URL            Postgres connection string (required for cleanup)
#   ARCHIVE_BUCKET          s3://bucket/prefix for audit logs and exports
#   AUDIT_LOG_DIR           local audit log directory (default /var/log/atomicip/audit)
#   RETENTION_DAYS          orphan / expiry grace period (default 30)
#   COMPRESS_AFTER_DAYS     compress data older than this (default 7)
#   COLD_TIER_AFTER_DAYS    move to infrequent-access storage after this (default 90)
#   STORAGE_COST_PER_GB     $/GB-month hot storage (default 0.023)
#   COLD_COST_PER_GB        $/GB-month cold storage (default 0.0125)
#   COST_LEDGER             savings ledger (default /var/lib/atomicip/cost-ledger.jsonl)
#   DRY_RUN=1               report what would be done without changing anything
set -euo pipefail

RETENTION_DAYS="${RETENTION_DAYS:-30}"
COMPRESS_AFTER_DAYS="${COMPRESS_AFTER_DAYS:-7}"
COLD_TIER_AFTER_DAYS="${COLD_TIER_AFTER_DAYS:-90}"
STORAGE_COST_PER_GB="${STORAGE_COST_PER_GB:-0.023}"
COLD_COST_PER_GB="${COLD_COST_PER_GB:-0.0125}"
AUDIT_LOG_DIR="${AUDIT_LOG_DIR:-/var/log/atomicip/audit}"
COST_LEDGER="${COST_LEDGER:-/var/lib/atomicip/cost-ledger.jsonl}"
DRY_RUN="${DRY_RUN:-0}"

log() { printf '[cost-optimize] %s\n' "$*" >&2; }

run() {
  if [ "$DRY_RUN" = "1" ]; then log "DRY_RUN: $*"; else "$@"; fi
}

# record <action> <target> <items> <bytes_saved> <monthly_usd_saved>
record() {
  mkdir -p "$(dirname "$COST_LEDGER")"
  jq -cn --arg ts "$(date -u +%FT%TZ)" --arg action "$1" --arg target "$2" \
    --argjson items "$3" --argjson bytes "$4" --argjson usd "$5" \
    --argjson dry "$([ "$DRY_RUN" = "1" ] && echo true || echo false)" \
    '{ts:$ts, action:$action, target:$target, items:$items, bytes_saved:$bytes, monthly_usd_saved:$usd, dry_run:$dry}' \
    >> "$COST_LEDGER"
  push_metric "$1" "$2" "$4" "$5"
}

push_metric() {
  [ -n "${PUSHGATEWAY_URL:-}" ] || return 0
  cat <<METRICS | curl -fsS --data-binary @- \
    "${PUSHGATEWAY_URL}/metrics/job/cost_optimization/action/$1/target/$2" >/dev/null || log "pushgateway unreachable"
# TYPE atomicip_cost_bytes_reclaimed gauge
atomicip_cost_bytes_reclaimed $3
# TYPE atomicip_cost_monthly_usd_saved gauge
atomicip_cost_monthly_usd_saved $4
# TYPE atomicip_cost_last_run_timestamp_seconds gauge
atomicip_cost_last_run_timestamp_seconds $(date +%s)
METRICS
}

usd_for_bytes() { # bytes rate_per_gb
  awk -v b="$1" -v r="$2" 'BEGIN { printf "%.4f", b / 1073741824 * r }'
}

psql_q() { psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -At -c "$1"; }

# ---------------------------------------------------------------- cleanup ---

# Each entry: target|size-estimate relation|WHERE clause selecting orphaned rows.
ORPHAN_RULES=(
  "expired_sessions|sessions|expires_at < now() - interval '${RETENTION_DAYS} days'"
  "orphaned_webhook_deliveries|webhook_deliveries|NOT EXISTS (SELECT 1 FROM webhooks w WHERE w.id = webhook_deliveries.webhook_id)"
  "stale_webhook_deliveries|webhook_deliveries|delivered_at < now() - interval '${RETENTION_DAYS} days'"
  "expired_recovery_tokens|recovery_tokens|expires_at < now() - interval '1 day'"
  "orphaned_watchlist_entries|watchlist|created_at < now() - interval '${RETENTION_DAYS} days' AND NOT EXISTS (SELECT 1 FROM ip_index i WHERE i.ip_id = watchlist.ip_id)"
)

cleanup_db() {
  : "${DATABASE_URL:?DATABASE_URL is required for cleanup}"
  local rule target table where exists rows avg_row bytes
  for rule in "${ORPHAN_RULES[@]}"; do
    IFS='|' read -r target table where <<< "$rule"
    exists=$(psql_q "SELECT to_regclass('public.${table}') IS NOT NULL")
    if [ "$exists" != "t" ]; then log "skip $target: table $table not present"; continue; fi

    rows=$(psql_q "SELECT count(*) FROM ${table} WHERE ${where}")
    [ "$rows" -gt 0 ] || { log "$target: nothing to clean"; continue; }
    avg_row=$(psql_q "SELECT COALESCE(pg_total_relation_size('${table}') / NULLIF((SELECT count(*) FROM ${table}), 0), 0)")
    bytes=$(( rows * avg_row ))

    log "$target: deleting $rows rows (~$bytes bytes)"
    if [ "$DRY_RUN" != "1" ]; then
      # Batched deletes keep lock times and WAL bursts small.
      while :; do
        deleted=$(psql_q "WITH d AS (DELETE FROM ${table} WHERE ctid IN (SELECT ctid FROM ${table} WHERE ${where} LIMIT 5000) RETURNING 1) SELECT count(*) FROM d")
        [ "$deleted" -gt 0 ] || break
      done
      psql_q "VACUUM (ANALYZE) ${table}" >/dev/null
    fi
    record cleanup "$target" "$rows" "$bytes" "$(usd_for_bytes "$bytes" "$STORAGE_COST_PER_GB")"
  done
}

cleanup_objects() {
  [ -n "${ARCHIVE_BUCKET:-}" ] || { log "ARCHIVE_BUCKET unset; skipping object cleanup"; return 0; }
  # Commitment exports (GET /ip/export) are ephemeral downloads.
  local cutoff listing count bytes
  cutoff=$(date -u -d "-${RETENTION_DAYS} days" +%F)
  listing=$(aws s3 ls "${ARCHIVE_BUCKET%/}/exports/" --recursive | awk -v c="$cutoff" '$1 < c')
  count=$(printf '%s' "$listing" | grep -c . || true)
  bytes=$(printf '%s\n' "$listing" | awk '{s += $3} END {print s + 0}')
  [ "$count" -gt 0 ] || { log "exports: nothing to clean"; return 0; }
  log "exports: removing $count objects ($bytes bytes)"
  printf '%s\n' "$listing" | awk '{print $4}' | while read -r key; do
    run aws s3 rm "s3://$(echo "${ARCHIVE_BUCKET#s3://}" | cut -d/ -f1)/${key}" --only-show-errors
  done
  record cleanup stale_exports "$count" "$bytes" "$(usd_for_bytes "$bytes" "$STORAGE_COST_PER_GB")"
}

# --------------------------------------------------------------- compress ---

compress_audit_logs() {
  [ -d "$AUDIT_LOG_DIR" ] || { log "no audit log dir at $AUDIT_LOG_DIR"; return 0; }
  local count=0 saved=0 f before after
  # The active log file is never touched; only rotated files (*.log.N / dated).
  while IFS= read -r -d '' f; do
    before=$(stat -c %s "$f")
    if [ "$DRY_RUN" = "1" ]; then
      after=$(zstd -19 -c "$f" | wc -c)
    else
      zstd -19 -q --rm "$f" -o "$f.zst"
      # Audit logs are HMAC-chained (AUDIT_HMAC_KEY): verify the round-trip
      # before trusting the compressed copy.
      zstd -t -q "$f.zst"
      after=$(stat -c %s "$f.zst")
    fi
    count=$((count + 1)); saved=$((saved + before - after))
  done < <(find "$AUDIT_LOG_DIR" -type f -name '*.log.*' ! -name '*.zst' ! -name '*.gz' \
             -mtime +"$COMPRESS_AFTER_DAYS" -print0)
  [ "$count" -gt 0 ] || { log "audit logs: nothing to compress"; return 0; }
  log "audit logs: compressed $count files, saved $saved bytes"
  record compress audit_logs "$count" "$saved" "$(usd_for_bytes "$saved" "$STORAGE_COST_PER_GB")"
}

compress_db_partitions() {
  [ -n "${DATABASE_URL:-}" ] || return 0
  # Postgres 14+: switch old partitions of append-only tables to lz4 TOAST
  # compression and rewrite them. Partition naming: <table>_yYYYYmMM.
  local parts p before after count=0 saved=0
  parts=$(psql_q "
    SELECT c.relname FROM pg_inherits i
      JOIN pg_class c ON c.oid = i.inhrelid
      JOIN pg_class p ON p.oid = i.inhparent
     WHERE p.relname IN ('audit_events', 'chain_events')
       AND to_date(substring(c.relname from 'y([0-9]{4}m[0-9]{2})$'), 'YYYY\"m\"MM')
           < date_trunc('month', now() - interval '${COMPRESS_AFTER_DAYS} days')
       AND NOT EXISTS (SELECT 1 FROM pg_description d WHERE d.objoid = c.oid AND d.description = 'compressed')" 2>/dev/null || true)
  for p in $parts; do
    before=$(psql_q "SELECT pg_total_relation_size('$p')")
    if [ "$DRY_RUN" != "1" ]; then
      psql_q "ALTER TABLE $p ALTER COLUMN payload SET COMPRESSION lz4" >/dev/null
      psql_q "VACUUM (FULL, ANALYZE) $p" >/dev/null
      psql_q "COMMENT ON TABLE $p IS 'compressed'" >/dev/null
    fi
    after=$(psql_q "SELECT pg_total_relation_size('$p')")
    count=$((count + 1)); saved=$((saved + before - after))
  done
  [ "$count" -gt 0 ] || return 0
  record compress db_partitions "$count" "$saved" "$(usd_for_bytes "$saved" "$STORAGE_COST_PER_GB")"
}

tier_archive() {
  [ -n "${ARCHIVE_BUCKET:-}" ] || return 0
  # Ensure the bucket moves old audit archives to cheaper storage classes.
  # Object lock (retention) is unaffected by storage-class transitions.
  local bucket prefix bytes
  bucket=$(echo "${ARCHIVE_BUCKET#s3://}" | cut -d/ -f1)
  prefix=$(echo "${ARCHIVE_BUCKET#s3://}" | cut -s -d/ -f2-)
  run aws s3api put-bucket-lifecycle-configuration --bucket "$bucket" --lifecycle-configuration "$(jq -cn \
    --arg p "${prefix:+$prefix/}audit/" --argjson d "$COLD_TIER_AFTER_DAYS" --argjson g "$((COLD_TIER_AFTER_DAYS * 4))" \
    '{Rules:[{ID:"atomicip-audit-tiering",Status:"Enabled",Filter:{Prefix:$p},
       Transitions:[{Days:$d,StorageClass:"STANDARD_IA"},{Days:$g,StorageClass:"GLACIER_IR"}]},
      {ID:"atomicip-exports-expiry",Status:"Enabled",Filter:{Prefix:"exports/"},Expiration:{Days:30}}]}')"
  bytes=$(aws s3 ls "${ARCHIVE_BUCKET%/}/audit/" --recursive --summarize | awk '/Total Size/ {print $3}')
  bytes="${bytes:-0}"
  record tier audit_archive 0 0 "$(awk -v b="$bytes" -v h="$STORAGE_COST_PER_GB" -v c="$COLD_COST_PER_GB" \
    'BEGIN { printf "%.4f", b / 1073741824 * (h - c) }')"
}

# ----------------------------------------------------------------- report ---

report() {
  local days="${REPORT_DAYS:-30}" since out
  since=$(date -u -d "-${days} days" +%FT%TZ)
  [ -f "$COST_LEDGER" ] || { log "no ledger at $COST_LEDGER"; return 0; }
  out="${REPORT_OUT:-cost-report-$(date -u +%F).md}"
  jq -rs --arg since "$since" --arg days "$days" '
    map(select(.ts >= $since and (.dry_run | not))) as $e
    | ($e | group_by(.action + "/" + .target)
          | map({key: (.[0].action + "/" + .[0].target),
                 runs: length,
                 items: (map(.items) | add),
                 gb: ((map(.bytes_saved) | add) / 1073741824),
                 usd: (map(.monthly_usd_saved) | add)})) as $rows
    | "# AtomicIP cost optimization report\n\n"
      + "Window: last \($days) days (since \($since))\n\n"
      + "| Action / target | Runs | Items | GB reclaimed | Est. $/month saved |\n"
      + "|---|---:|---:|---:|---:|\n"
      + ($rows | map("| \(.key) | \(.runs) | \(.items) | \(.gb * 100 | round / 100) | \(.usd * 100 | round / 100) |") | join("\n"))
      + "\n\n**Total reclaimed:** \(($e | map(.bytes_saved) | add // 0) / 1073741824 * 100 | round / 100) GB  \n"
      + "**Estimated monthly savings:** $\(($e | map(.monthly_usd_saved) | add // 0) * 100 | round / 100)\n"
  ' "$COST_LEDGER" > "$out"
  log "report written to $out"
  cat "$out"
}

case "${1:-all}" in
  cleanup)  cleanup_db; cleanup_objects ;;
  compress) compress_audit_logs; compress_db_partitions; tier_archive ;;
  report)   report ;;
  all)      cleanup_db; cleanup_objects; compress_audit_logs; compress_db_partitions; tier_archive; report ;;
  *) echo "usage: $0 {cleanup|compress|report|all}" >&2; exit 2 ;;
esac
