# Performance Tuning Guide

> Closes #1059. Guidance for operators and contributors on optimizing Atomic Patent:
> contract gas/resource usage, API server caching, scaling, and performance monitoring.

Related: [Soroban Resource Limits](soroban-resource-limits.md) ·
[Benchmarks](PERFORMANCE.md) · [Architecture](architecture.md)

---

## 1. Performance Optimization Techniques

### Where time and cost go

| Layer | Dominant cost | Main levers |
|-------|---------------|-------------|
| Soroban contracts | CPU instructions, ledger reads/writes, TTL rent, tx size | Fewer storage entries, batching, lean events |
| API server | Soroban RPC round-trips, serialization, signature checks | Caching, connection pooling, compression |
| Clients | Round-trips, polling | Batch endpoints, WebSocket/SSE instead of polling |

### General techniques
1. **Measure first.** Use the benchmarks in `contracts/*/src/benchmarks.rs` and
   `scripts/loadTest.js` before and after every change.
2. **Batch.** Prefer `batch_commit_ip`, `batch_initiate_swap`, and `POST /batch`
   over many single calls. Each Soroban transaction has fixed overhead.
3. **Push, don't poll.** Subscribe via `/ws`, `/graphql/ws` or `/events` instead of
   polling `GET /v1/swap/{id}`.
4. **Paginate with cursors.** Use `/v1/ip/owner/{owner}/cursor` for large owners.
5. **Compress.** Response compression is enabled in the API server; clients should send
   `Accept-Encoding: gzip, br`. Batch payloads compress ~10× at 50+ swaps
   (see [PERFORMANCE.md](PERFORMANCE.md)).

## 2. Gas & Resource Optimization

Soroban charges for CPU instructions, memory, ledger entry reads/writes, bytes
read/written, event bytes and storage rent (TTL). Per-transaction limits are listed in
[soroban-resource-limits.md](soroban-resource-limits.md).

### Contract-level tips

| Tip | Why | Example in codebase |
|-----|-----|---------------------|
| Use `instance()` storage for small global config; `persistent()` for per-record data | Instance storage is loaded once per invocation | `DataKey::Admin` in instance storage |
| Minimise ledger entries per operation | Read/write count is capped (see limits doc) and billed | One `SwapRecord` per swap; indexes (`SellerSwaps`, `BuyerSwaps`) only where queried |
| Extend TTL only on write paths | `extend_ttl` is a write; avoid on hot read paths | `LEDGER_BUMP` applied when saving records |
| Store hashes, not blobs | Bytes written are billed; tx size ≤ 100 KB | Dispute evidence stored as `BytesN<32>` hashes |
| Keep events compact | Event bytes are billed | `symbol_short!` topics (`disputed`, `disp_res`) |
| Fail fast | Cheaper to panic before storage writes / token transfers | Auth + status checks at the top of each entrypoint |
| Avoid O(n²) loops over large `Vec`s | CPU instructions scale with n² | Keep committee/signer lists small (duplicate checks are quadratic) |
| Respect batch caps | Stay well inside the 100M CPU and 1 000 entry limits | `MAX_BATCH_SIZE = 50` |

### Build settings

The workspace release profile is already tuned for size (`Cargo.toml`):

```toml
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = "symbols"
debug = 0
```

Smaller WASM means cheaper upload and less instantiation cost. Track it with:

```bash
./scripts/build.sh
./scripts/check-wasm-size.sh     # compares against scripts/wasm-size-baseline.json
```

Optionally run `soroban contract optimize --wasm <file>` for further size reduction.

### Measuring cost

```bash
# Simulate a call and read the resource footprint without submitting
stellar contract invoke --id <CONTRACT_ID> --source <KEY> --network testnet \
  --sim-only -- commit_ip --owner <ADDR> --commitment_hash <HASH>

# Budget smoke test used in CI
./scripts/smoke-test-budget.sh
```

In unit tests, `env.budget().reset_default()` / `env.budget().print()` show CPU and
memory use for a code path.

### Client-side fee tips
- Always simulate before submitting and use the simulated resource fee plus a small
  margin, rather than a large fixed fee.
- Bundle related operations in one batch call where the contract supports it.

## 3. Caching Strategies

The API server cache (`api-server/src/cache.rs`) is Redis-backed when `REDIS_URL` is
set, with an in-process `DashMap` fallback.

| Data | TTL | Notes |
|------|-----|-------|
| IP records | 60 s | IP records change rarely (transfer/revoke) |
| Swaps | 30 s | Status changes frequently during a swap |
| Reputation | 300 s | Aggregated, tolerant of staleness |
| Default | 30 s | Anything else |

### Recommendations
1. **Always configure `REDIS_URL` in multi-replica deployments.** Without Redis each
   replica caches independently, so invalidations on one replica are not seen by
   others and clients may read stale data for up to one TTL.
2. **Watch degradation.** `cache_backend_degraded_transitions_total` increments on
   every Redis ↔ in-process switch. A background health check re-attempts Redis every
   10 s. Alert if the counter increases.
3. **Invalidate on write.** Write handlers invalidate the affected keys
   (`invalidate`, `invalidate_prefix`, `invalidate_pattern`); new handlers must do the same.
4. **Tune TTLs to your workload.** Read-heavy marketplaces can raise IP TTLs; latency-
   sensitive swap UIs should rely on WebSocket events rather than lower TTLs.
5. **Size Redis** for the working set: `maxmemory` with `allkeys-lru` eviction.
6. **Client caching** — cache immutable data (commitment hashes, completed swaps)
   indefinitely on the client; respect `Cache-Control` for everything else.

## 4. Scaling Guidelines

### API server
- **Stateless horizontal scaling.** Run multiple `api-server` replicas behind a load
  balancer. Shared state (cache, rate limits) must go through Redis so limits and
  invalidations are global.
- **Connection pooling** (`connection_pool.rs`) defaults: min 5, max 20 connections,
  30 s connection timeout, 300 s idle timeout, 1 800 s max lifetime. Raise `max_size`
  when `db_pool_waiting_requests` is persistently > 0; lower it if the backend is
  connection-limited.
- **Circuit breaker** (`circuit_breaker.rs`) defaults: opens after 5 failures, stays
  open 30 s, closes after 2 successes, 3 half-open probes. Tighten for fast failover,
  loosen if the RPC has transient blips.
- **Rate limiting** — Free / Premium / Enterprise tiers, token bucket, enforced
  atomically across global, IP and user scopes. Use the Redis store for multi-replica.
- **Request queue & dedup** (`request_queue.rs`, `deduplication.rs`) smooth bursts and
  drop duplicate submits — keep them enabled under load.

### Soroban RPC
- Use a dedicated or self-hosted RPC (`SOROBAN_RPC_URL`) in production; public
  endpoints are rate-limited.
- Keep the RPC geographically close to the API servers.
- Configure multiple RPC endpoints behind the load balancer / fallback module for
  resilience.

### Capacity planning

| Load | Suggested topology |
|------|--------------------|
| Dev / testnet | 1 replica, in-process cache |
| < 50 req/s | 2 replicas, single Redis, shared RPC |
| 50–500 req/s | 3–6 replicas, Redis with replica, dedicated RPC |
| > 500 req/s | Autoscaling replicas (CPU 60% target), Redis cluster, multiple RPC nodes |

Validate each step with `node scripts/loadTest.js` against a staging environment.

## 5. Performance Monitoring Setup

### Metrics
`GET /metrics` exposes Prometheus metrics. Key series:

| Metric | Type | Use |
|--------|------|-----|
| `http_requests_total` | counter | Throughput by route/status |
| `http_request_duration_seconds` | histogram | Latency percentiles |
| `http_errors_total` | counter | Error rate |
| `db_pool_active_connections` / `db_pool_idle_connections` | gauge | Pool saturation |
| `db_pool_waiting_requests` | gauge | Pool contention |
| `cache_backend_degraded_transitions_total` | counter | Redis health |

Example Prometheus scrape config:

```yaml
scrape_configs:
  - job_name: atomicip-api
    scrape_interval: 15s
    static_configs:
      - targets: ['api-server:8080']
```

### Suggested alerts

```yaml
groups:
  - name: atomicip
    rules:
      - alert: HighP95Latency
        expr: histogram_quantile(0.95, sum(rate(http_request_duration_seconds_bucket[5m])) by (le)) > 1
        for: 10m
      - alert: HighErrorRate
        expr: sum(rate(http_errors_total[5m])) / sum(rate(http_requests_total[5m])) > 0.05
        for: 5m
      - alert: PoolContention
        expr: db_pool_waiting_requests > 0
        for: 5m
      - alert: CacheDegraded
        expr: increase(cache_backend_degraded_transitions_total[10m]) > 0
```

### Tracing
Enable OpenTelemetry to trace requests through to Soroban RPC:

```bash
OTEL_ENABLED=true
OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4317
OTEL_SERVICE_NAME=atomicip-api
```

### Health checks
- `GET /health` — liveness probe.
- `GET /health/detailed` — readiness probe including dependencies.

### Logs
The server emits structured JSON logs (method, path, status, duration, client IP);
ship them to your log platform and build latency/error dashboards alongside metrics.

### Performance targets (SLO starting points)

| Endpoint class | p95 latency | Error rate |
|----------------|-------------|------------|
| Cached reads (`GET /v1/ip/*`, `GET /v1/swap/*`) | < 100 ms | < 0.1% |
| Writes (commit, swap actions) | < 2 s (dominated by ledger close) | < 1% |
| Batch endpoints (50 items) | < 5 s | < 1% |

## 6. Tuning Checklist

- [ ] `REDIS_URL` set for all replicas; degradation alert configured
- [ ] Pool size tuned against `db_pool_waiting_requests`
- [ ] Circuit breaker thresholds reviewed for your RPC
- [ ] Clients use batch endpoints and push notifications
- [ ] WASM size within baseline; release profile unchanged
- [ ] Prometheus scraping `/metrics`; alerts deployed
- [ ] OpenTelemetry enabled in staging/production
- [ ] Load test run after every significant change
