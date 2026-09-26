# Monitoring

| Path | Purpose | Issue |
|------|---------|-------|
| `prometheus/commitment-lifecycle-rules.yml` | Recording + alert rules for commitment states, transition rates, anomalies and week-over-week trends | #1064 |
| `grafana/commitment-lifecycle-dashboard.json` | Dashboard with an overview row, one row per state (active / revealed / archived), trends and alert-pipeline panels | #1064 |
| `alertmanager/alertmanager.yml` | Grouping (dedup), inhibition (correlation), severity routing (escalation) | #1065 |

Metrics are exported on the api-server `/metrics` endpoint by
`api-server/src/commitment_monitoring.rs` and `api-server/src/alerting.rs`.
The api-server also exposes JSON views at `GET /v1/admin/commitments/lifecycle`
and `GET /v1/admin/alerts`.

See [`docs/alerting-best-practices.md`](../docs/alerting-best-practices.md) and
[`docs/disaster-recovery.md`](../docs/disaster-recovery.md).
