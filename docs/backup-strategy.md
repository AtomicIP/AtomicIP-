# Backup Strategy and Verification

> Issue #1066. Owner: Platform on-call. Backup schedule and RTO/RPO targets are
> defined in [disaster-recovery.md](disaster-recovery.md) §2–3; this page
> covers how we **prove** those backups are restorable.

A backup that has never been restored is a hope, not a backup. Every day the
latest backup is restored into a throwaway Postgres and checked end to end.

## 1. What is backed up

| Asset | Method | Frequency | Retention |
|-------|--------|-----------|-----------|
| Postgres | Base backup (`pg_basebackup`, with `backup_manifest` + `SHA256SUMS`) + continuous WAL archive | Base daily 02:00 UTC, WAL continuous | 35 days PITR, monthly 1 year |
| Audit log | Append-only sync to object-locked bucket (zstd) | 15 min | 7 years |
| Secrets, admin keys | Secret-manager replication; Shamir 3-of-5 | On change | Permanent |
| Contract state | Stellar ledger (not backed up by us) | — | — |
| Deployment state | Git (`deploy/`, GitOps #1069) | Every change | Permanent |

Expected bucket layout (`BACKUP_BUCKET`):

```
postgres/base/<YYYYMMDDTHHMMSSZ>/{base.tar.gz, pg_wal.tar.gz, backup_manifest, SHA256SUMS}
postgres/wal/<segment files>
audit/<YYYY>/<MM>/<DD>/*.log.zst
```

Encryption: SSE-KMS at rest, cross-region replication to the DR region.

## 2. Verification pipeline

`scripts/ops/backup-verify.sh` runs:

| Step | Check | Fails when |
|------|-------|------------|
| Freshness | Age of newest base backup, WAL segment and audit archive | Base > 26 h, WAL > 5 min, audit > 30 min |
| Integrity | `sha256sum -c SHA256SUMS`, `pg_verifybackup` against the manifest | Any checksum / manifest mismatch |
| Restore | Base backup + WAL replay (`recovery_target_timeline=latest`) in a disposable container | Postgres exits, or recovery exceeds RTO |
| Validation | Expected tables exist, row counts recorded, `amcheck` on every btree index | Missing table, index corruption |
| Audit archive | Newest archive decompresses (`zstd -t` / `gzip -t`) | Corrupt archive |
| RTO | Restore duration vs `RTO_TARGET_SECONDS` (2 h) | Slower than target |

Where it runs:

* **Daily, in-cluster**: `deploy/k8s/jobs/backup-verification-cronjob.yaml` at 04:30 UTC.
* **Weekly, off-cluster**: `.github/workflows/backup-verification.yml` (Monday 06:00 UTC)
  restores from a completely separate environment, proving recoverability even
  if the production cluster is lost. It uploads the JSON report (kept 90 days)
  and opens a GitHub issue on failure.

## 3. Metrics

Pushed to the Pushgateway (`job="backup_verification"`):

* `atomicip_backup_verify_success` (1/0), `atomicip_backup_verify_duration_seconds`
* `atomicip_backup_verify_last_success_timestamp_seconds`
* `atomicip_backup_restore_seconds` — measured recovery time
* `atomicip_backup_rpo_seconds` — gap between run start and last committed transaction in the restore
* `atomicip_backup_age_seconds{kind}`, `atomicip_backup_restored_rows{table}`

Alerts (`monitoring/prometheus/operations-rules.yml`): `BackupVerificationFailed`
(critical), `BackupVerificationStale` (critical, no success in 48 h), `BackupTooOld`
(warning), `BackupRestoreSlowerThanRTO` (warning). On failure the script also
posts `BackupVerificationFailed` directly to Alertmanager, so an alert fires even
if the Pushgateway is down; critical alerts open an incident (#1068).

## 4. Recovery time

Track `atomicip_backup_restore_seconds` over time on the operations dashboard.
If it trends toward the 2 h RTO:

1. Take base backups more often (fewer WAL segments to replay).
2. Restore from the same region as the backup bucket; use a larger restore instance.
3. Use parallel WAL prefetch (`pgBackRest`/`wal-g` with `--prefetch`).

## 5. When verification fails

1. The alert opens an incident (SEV2). Acknowledge it
   ([incident-response.md](incident-response.md)).
2. Read the report (`backup-verification.json` in the workflow artifact or the CronJob logs:
   `kubectl -n atomicip-production logs job/<backup-verification-…> -c verify`).
3. By failure type:
   * **Stale backup** – check the backup job and WAL archiver (`pg_stat_archiver.failed_count`).
   * **Checksum / manifest** – mark that backup bad; verify the previous one with
     `BACKUP_BUCKET=… scripts/ops/backup-verify.sh` pointed at it; take a fresh base backup now.
   * **Restore / amcheck** – treat as possible corruption in production; run `amcheck` on the primary.
   * **RTO exceeded** – see §4; not an immediate data-loss risk.
4. Pause destructive jobs (cost cleanup, #1067) until a verification passes.
5. Close the incident only after a green verification run.

## 6. Manual run

```bash
BACKUP_BUCKET=s3://atomicip-backups/prod \
PG_IMAGE=postgres:16 KEEP_WORKDIR=1 \
scripts/ops/backup-verify.sh
```

Requirements: `aws` CLI with read access to the bucket, Docker, `jq`, `zstd`.

## 7. Review

Backup strategy is reviewed quarterly with the DR plan. Each quarterly DR drill
([disaster-recovery.md](disaster-recovery.md)) uses the verification output as its
starting point.
