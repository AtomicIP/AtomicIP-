# Cost Optimization Automation

> Issue #1067. Owner: Platform. Everything here is automated; this page explains
> what runs, what it touches and how savings are reported.

| Lever | Implementation | Schedule |
|-------|----------------|----------|
| Auto-scaling | `deploy/k8s/base/hpa.yaml` | Continuous |
| Orphaned data cleanup | `scripts/ops/cost-optimize.sh cleanup` | Nightly 03:00 UTC (CronJob) |
| Compression & storage tiering | `scripts/ops/cost-optimize.sh compress` | Nightly 03:00 UTC (CronJob) |
| Savings tracking | JSONL ledger + Pushgateway metrics | Every run |
| Cost report | `.github/workflows/cost-report.yml` | Weekly, Monday 07:00 UTC |

## 1. Auto-scaling

The api-server is stateless (see [disaster-recovery.md](disaster-recovery.md) §1),
so it scales horizontally with a `HorizontalPodAutoscaler`:

* Targets: 70 % CPU, 80 % memory of **requests** (100m CPU / 128Mi).
* Scale-up: up to +100 % per minute after a 60 s window.
* Scale-down: after 5 min of low load, at most one pod per minute (no flapping).
* Bounds: staging 1–3, production 3–20 (set in each overlay; change via PR, see [gitops.md](gitops.md)).
* A `PodDisruptionBudget` keeps one pod serving during node scale-down, letting
  the cluster autoscaler remove idle nodes safely.
* `ApiServerAtMaxReplicas` fires if the HPA is pinned at its ceiling for 30 min.

Right-sizing: review `atomicip:api_replicas:avg1d` and container CPU/memory
usage monthly; lower `requests` when p95 usage stays below 50 % of them.

## 2. Orphaned data cleanup

Only off-chain caches/indexes are cleaned; the Stellar ledger is never touched.

| Target | Rule |
|--------|------|
| `expired_sessions` | `sessions.expires_at` older than `RETENTION_DAYS` (30) |
| `orphaned_webhook_deliveries` | Deliveries whose webhook was unregistered |
| `stale_webhook_deliveries` | Delivered more than `RETENTION_DAYS` ago |
| `expired_recovery_tokens` | Expired > 1 day ago |
| `orphaned_watchlist_entries` | Watchlist rows for IPs no longer indexed |
| `stale_exports` | `GET /ip/export` files in object storage older than 30 days |

Rows are deleted in batches of 5 000, followed by `VACUUM (ANALYZE)`. Tables that
do not exist are skipped. Add rules to `ORPHAN_RULES` in the script.

## 3. Compression of old data

* **Audit logs**: rotated files older than `COMPRESS_AFTER_DAYS` (7) are
  compressed with `zstd -19` (typically 8–12× for JSON logs) and integrity-checked
  before the original is removed. The active log is never touched; the HMAC chain
  is preserved byte-for-byte inside the archive.
* **Postgres partitions** of append-only tables (`audit_events`, `chain_events`)
  older than a month switch to `lz4` TOAST compression and are rewritten.
* **Object storage tiering**: audit archives move to Standard-IA after
  `COLD_TIER_AFTER_DAYS` (90) and Glacier Instant Retrieval after 360 days;
  exports expire after 30 days. Object Lock retention is unaffected.

## 4. Savings tracking and reports

Each action appends to the ledger (`COST_LEDGER`, persisted on a PVC and synced to object storage):

```json
{"ts":"2026-09-27T03:00:12Z","action":"cleanup","target":"expired_sessions","items":18234,"bytes_saved":41943040,"monthly_usd_saved":0.0009,"dry_run":false}
```

and pushes `atomicip_cost_bytes_reclaimed`, `atomicip_cost_monthly_usd_saved`
and `atomicip_cost_last_run_timestamp_seconds` to the Pushgateway.
`scripts/ops/cost-optimize.sh report` aggregates the ledger into a Markdown
table; the weekly workflow adds actual spend per service from AWS Cost Explorer
(resources tagged `project=atomicip`) and publishes it as a run summary + artifact.

Savings estimates use `STORAGE_COST_PER_GB` / `COLD_COST_PER_GB`; update them when
provider pricing changes. Autoscaling savings are visible as replica-hours in
the `atomicip:api_replicas:avg1d` recording rule versus the fixed pre-HPA count.

## 5. Operating safely

* Always try `DRY_RUN=1 scripts/ops/cost-optimize.sh all` after changing rules.
* Cleanup never runs against a database that failed last night's backup
  verification (check `BackupVerificationFailed` before re-running manually).
* `CostOptimizationJobStale` (info) fires when no run is recorded for 3 days.
