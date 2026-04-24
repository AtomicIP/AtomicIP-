# API Server Issues Implementation TODO

## Execution Order
1. #260 Logging & Metrics (foundation, no deps)
2. #259 Rate Limiting (can be independent)
3. #261 Auth & Authorization (required before webhooks)
4. #262 Webhook Support (last, leverages auth)

---

## Issue #260 — Logging & Metrics (`blackboxai/issue-260-logging-metrics`)
- [ ] Create branch
- [ ] Add dependencies to `api-server/Cargo.toml`
- [ ] Create `api-server/src/metrics.rs` (Prometheus + structured logging)
- [ ] Modify `api-server/src/main.rs` (init tracing, /metrics route, TraceLayer)
- [ ] Modify `api-server/src/handlers.rs` (tracing::instrument)
- [ ] Commit and push

## Issue #259 — Rate Limiting (`blackboxai/issue-259-rate-limiting`)
- [ ] Create branch
- [ ] Add dependencies to `api-server/Cargo.toml`
- [ ] Create `api-server/src/rate_limit.rs`
- [ ] Modify `api-server/src/main.rs` (mount RateLimitLayer)
- [ ] Commit and push

## Issue #261 — Auth & Authorization (`blackboxai/issue-261-auth`)
- [ ] Create branch
- [ ] Add dependencies to `api-server/Cargo.toml`
- [ ] Create `api-server/src/auth.rs`
- [ ] Modify `api-server/src/main.rs` (auth routes + middleware)
- [ ] Modify `api-server/src/handlers.rs` (login/refresh)
- [ ] Modify `api-server/src/schemas.rs` (auth schemas)
- [ ] Commit and push

## Issue #262 — Webhook Support (`blackboxai/issue-262-webhooks`)
- [ ] Create branch
- [ ] Add dependencies to `api-server/Cargo.toml`
- [ ] Create `api-server/src/webhook.rs`
- [ ] Modify `api-server/src/main.rs` (webhook routes)
- [ ] Modify `api-server/src/handlers.rs` (register/unregister + triggers)
- [ ] Modify `api-server/src/schemas.rs` (webhook schemas)
- [ ] Commit and push

---

**Status:** In progress

