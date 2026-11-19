# Monitoring

Operational notes for the Prometheus setup. The configuration reference lives in
[`monitoring/README.md`](../monitoring/README.md); this page covers what to do
when an alert fires.

## Metrics endpoint

<a id="metrics-endpoint"></a>

**Alert:** `ApiServerMetricsScrapeFailing` (`up{job="atomicip-api-metrics"} == 0` for 10m)

Prometheus cannot scrape the api-server internal metrics port. The public
`/metrics` route is no longer served (see
[Protecting the `/metrics` endpoint](../monitoring/README.md#protecting-the-metrics-endpoint-1207)),
so this is almost always a configuration or network problem.

1. Open the Prometheus **Targets** page and read the last scrape error for
   `atomicip-api-metrics`.
2. `401 Unauthorized`: the bearer token Prometheus reads from
   `/etc/prometheus/secrets/metrics_auth_token` does not match the api-server's
   `METRICS_AUTH_TOKEN`. Re-sync the secret from the same source as the
   api-server and reload Prometheus.
3. `connection refused` or timeout: confirm the api-server pod is running and
   listening on `METRICS_PORT` (default `9090`), and that no network policy blocks
   Prometheus from that port.
4. `404 Not Found`: the scrape path is wrong. It must be `/metrics` on the
   internal port.
5. If the api-server is down, follow the API server incident process in
   [`incident-response.md`](incident-response.md) instead; the scrape failure is
   a symptom.
6. Once `up` returns to `1`, the alert resolves automatically.
