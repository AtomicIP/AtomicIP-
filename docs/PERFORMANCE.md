
## Slow request visibility

The API server records every request in Prometheus as
`http_request_duration_seconds`. Requests at or above the configured slow
request threshold also increment `http_slow_requests_total` and emit a
structured `slow request detected` warning containing the method, path, status,
latency, and threshold.

The default threshold is 1,000 ms. Set `SLOW_REQUEST_THRESHOLD_MS` to a
positive number of milliseconds when starting the server:

```sh
SLOW_REQUEST_THRESHOLD_MS=500 cargo run --manifest-path api-server/Cargo.toml
```

Example Prometheus query:

```promql
sum by (path) (rate(http_slow_requests_total[5m]))
```

### batchCompressor baseline — 2026-09-04

> Node v24.14.0 · 200 iterations per scenario · zlib deflate (Node built-in)

| Scenario | Raw (bytes) | Compressed (bytes) | Ratio | Compress (ms/op) | Decompress (ms/op) |
|----------|-------------|-------------------|-------|------------------|--------------------|
| tiny  (  1 swap ) | 244 | 135 | 1.81× | 0.025 | 0.013 |
| small ( 10 swaps) | 2425 | 343 | 7.07× | 0.058 | 0.019 |
| mid   ( 50 swaps) | 12157 | 1187 | 10.24× | 0.098 | 0.048 |
| large (100 swaps) | 24325 | 2239 | 10.86× | 0.183 | 0.117 |
