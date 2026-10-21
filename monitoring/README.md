# Monitoring

| Path | Purpose | Issue |
|------|---------|-------|
| `prometheus/commitment-lifecycle-rules.yml` | Recording + alert rules for commitment states, transition rates, anomalies and week-over-week trends | #1064 |
| `grafana/commitment-lifecycle-dashboard.json` | Dashboard with an overview row, one row per state (active / revealed / archived), trends and alert-pipeline panels | #1064 |
| `alertmanager/alertmanager.yml` | Grouping (dedup), inhibition (correlation), severity routing (escalation) | #1065 |
| `alertmanager/alertmanager.yml` (`incident-tracker`, `opsgenie-oncall`) | Critical alerts open incidents in the api-server; Opsgenie as alternative pager | #1068 |
| `prometheus/operations-rules.yml` | Backup verification, cost optimization / autoscaling, and Argo CD sync alerts | #1066, #1067, #1069 |

Metrics are exported on the api-server `/metrics` endpoint by
`api-server/src/commitment_monitoring.rs` and `api-server/src/alerting.rs`.
The api-server also exposes JSON views at `GET /v1/admin/commitments/lifecycle`
and `GET /v1/admin/alerts`.

## Protecting the `/metrics` endpoint (#1207)

`/metrics` is no longer a public route. It is served on a separate internal
metrics port (configured via `METRICS_PORT`, default `9090`) that should only be
reachable from the internal network / Prometheus scraper, and every scrape must
present the bearer token configured in `METRICS_AUTH_TOKEN`:

```
Authorization: Bearer $METRICS_AUTH_TOKEN
```

Requests without a valid token are refused with `401 Unauthorized`, and the
metrics port is not exposed through the public ingress.

### Prometheus scrape configuration

The Prometheus scrape job for the api-server has been updated to target the
internal metrics port and to send the bearer token. The token is read from a
file so it never appears in the committed config:

```yaml
scrape_configs:
  - job_name: api-server
    metrics_path: /metrics
    scheme: http
    authorization:
      type: Bearer
      credentials_file: /etc/prometheus/secrets/metrics_auth_token
    static_configs:
      - targets: ["api-server-internal:9090"]
```

Make sure the `metrics_auth_token` secret file is mounted into the Prometheus
pod and matches the api-server's `METRICS_AUTH_TOKEN`. Unauthenticated scrapes
are covered by a test in the api-server test suite.

See [`docs/alerting-best-practices.md`](../docs/alerting-best-practices.md),
[`docs/disaster-recovery.md`](../docs/disaster-recovery.md),
[`docs/incident-response.md`](../docs/incident-response.md) and
[`docs/backup-strategy.md`](../docs/backup-strategy.md).