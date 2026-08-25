//! Concurrent token-bucket rate limiting for HTTP requests.
//!
//! Limits are enforced atomically across global, source-IP, and authenticated
//! user scopes: a request can never consume one quota and then fail another
//! quota. The token-bucket accounting itself lives behind the [`RateLimitStore`]
//! trait so it can be backed either by process-local memory (the default,
//! [`InProcessStore`]) or by a shared Redis instance ([`RedisStore`]) so that
//! multiple replicas behind a load balancer enforce one combined quota per
//! client instead of one quota per replica. Everything else (tier lookup,
//! violation backoff, cardinality bounding) stays process-local in both
//! modes, since it does not need to be shared for the quota to be correct.

use crate::auth::AuthExtension;
use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderName, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RateLimitTier {
    Free,
    Premium,
    Enterprise,
}

impl RateLimitTier {
    fn as_str(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Premium => "premium",
            Self::Enterprise => "enterprise",
        }
    }
}

/// A bucket's sustained refill rate and maximum burst capacity.
#[derive(Debug, Clone, Copy)]
pub struct BucketQuota {
    pub requests_per_minute: u32,
    pub burst: u32,
}

impl BucketQuota {
    pub const fn new(requests_per_minute: u32, burst: u32) -> Self {
        Self {
            requests_per_minute,
            burst,
        }
    }

    fn refill_per_second(self) -> f64 {
        self.requests_per_minute as f64 / 60.0
    }
}

/// Where token-bucket counters are stored. See the module docs for how this
/// interacts with multi-instance deployments.
#[derive(Debug, Clone, Default)]
pub enum RateLimitBackend {
    /// Counters live in this process's memory only. Each replica enforces
    /// its own independent quota — **not safe for a multi-instance
    /// deployment**, where a client distributed across N replicas would
    /// effectively receive N times the documented quota. Use this only for
    /// tests and single-instance deployments.
    #[default]
    InProcess,
    /// Counters live in Redis at the given connection URL (e.g.
    /// `redis://127.0.0.1:6379`), shared by every replica that points at the
    /// same instance. Required for correct quota enforcement behind a load
    /// balancer running multiple replicas.
    Redis(String),
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub global: BucketQuota,
    pub per_ip: BucketQuota,
    pub free: BucketQuota,
    pub premium: BucketQuota,
    pub enterprise: BucketQuota,
    /// Honor forwarding headers only when the immediate peer is a trusted proxy.
    pub trust_proxy_headers: bool,
    pub base_backoff: Duration,
    pub max_backoff: Duration,
    /// Bounds attacker-controlled cardinality. New identities share overflow buckets.
    pub max_tracked_ips: usize,
    pub max_tracked_users: usize,
    pub idle_ttl: Duration,
    /// Where token-bucket counters are stored. Defaults to
    /// [`RateLimitBackend::InProcess`]; set to [`RateLimitBackend::Redis`]
    /// for correct enforcement across multiple replicas.
    pub backend: RateLimitBackend,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            global: BucketQuota::new(20_000, 2_000),
            per_ip: BucketQuota::new(300, 100),
            free: BucketQuota::new(60, 30),
            premium: BucketQuota::new(600, 200),
            enterprise: BucketQuota::new(6_000, 1_000),
            trust_proxy_headers: false,
            base_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(60),
            max_tracked_ips: 100_000,
            max_tracked_users: 100_000,
            idle_ttl: Duration::from_secs(15 * 60),
            backend: RateLimitBackend::InProcess,
        }
    }
}

impl RateLimitConfig {
    fn quota_for(&self, tier: RateLimitTier) -> BucketQuota {
        match tier {
            RateLimitTier::Free => self.free,
            RateLimitTier::Premium => self.premium,
            RateLimitTier::Enterprise => self.enterprise,
        }
    }
}

#[derive(Debug, Clone)]
struct Bucket {
    tokens: f64,
    updated_at: Instant,
    last_seen: Instant,
}

impl Bucket {
    fn full(quota: BucketQuota, now: Instant) -> Self {
        Self {
            tokens: quota.burst as f64,
            updated_at: now,
            last_seen: now,
        }
    }

    fn refill(&mut self, quota: BucketQuota, now: Instant) {
        let elapsed = now.saturating_duration_since(self.updated_at).as_secs_f64();
        self.tokens = (self.tokens + elapsed * quota.refill_per_second()).min(quota.burst as f64);
        self.updated_at = now;
        self.last_seen = now;
    }
}

/// Result of atomically refilling and (if every bucket had capacity)
/// consuming one token from a set of buckets.
#[derive(Debug, Clone)]
struct RefillResult {
    allowed: bool,
    /// Post-refill, pre-consumption token count for each bucket, aligned
    /// with the order the buckets were requested in.
    tokens: Vec<f64>,
}

/// Storage for token-bucket counters. Implementations must refill and, if
/// (and only if) every requested bucket has at least one token available,
/// consume one token from all of them as a single atomic step — a request
/// must never consume one quota and then fail another.
#[async_trait::async_trait]
trait RateLimitStore: Send + Sync + std::fmt::Debug {
    async fn refill_and_consume(
        &self,
        buckets: &[(String, BucketQuota)],
        now: Instant,
        idle_ttl: Duration,
    ) -> RefillResult;
}

/// Process-local counter storage. Not safe for multi-instance deployment —
/// see [`RateLimitBackend::InProcess`].
#[derive(Debug, Default)]
struct InProcessStore {
    buckets: Mutex<InProcessBuckets>,
}

#[derive(Debug, Default)]
struct InProcessBuckets {
    buckets: HashMap<String, Bucket>,
    checks: u64,
}

#[async_trait::async_trait]
impl RateLimitStore for InProcessStore {
    async fn refill_and_consume(
        &self,
        buckets: &[(String, BucketQuota)],
        now: Instant,
        idle_ttl: Duration,
    ) -> RefillResult {
        let mut store = self.buckets.lock().unwrap();
        store.checks += 1;
        if store.checks % 1024 == 0 {
            store
                .buckets
                .retain(|_, b| now.saturating_duration_since(b.last_seen) < idle_ttl);
        }

        let mut tokens = Vec::with_capacity(buckets.len());
        for (key, quota) in buckets {
            let bucket = store
                .buckets
                .entry(key.clone())
                .or_insert_with(|| Bucket::full(*quota, now));
            bucket.refill(*quota, now);
            tokens.push(bucket.tokens);
        }

        let allowed = tokens.iter().all(|t| *t >= 1.0);
        if allowed {
            for (key, _) in buckets {
                store.buckets.get_mut(key).unwrap().tokens -= 1.0;
            }
        }
        RefillResult { allowed, tokens }
    }
}

/// Refills and checks every bucket, then consumes from all of them, in a
/// single Lua script executed atomically by Redis. Time comes from Redis's
/// own `TIME` command rather than the caller's clock, so refill accounting
/// cannot be skewed by clock drift between application replicas.
const REFILL_AND_CONSUME_SCRIPT: &str = r#"
local n = tonumber(ARGV[1])
local ttl = tonumber(ARGV[2])
local time = redis.call('TIME')
local now = tonumber(time[1]) + tonumber(time[2]) / 1000000

local tokens = {}
for i = 1, n do
    local rate = tonumber(ARGV[2 + i])
    local burst = tonumber(ARGV[2 + n + i])
    local state = redis.call('HMGET', KEYS[i], 'tokens', 'updated_at')
    local cur = tonumber(state[1])
    local updated_at = tonumber(state[2])
    if cur == nil then
        cur = burst
        updated_at = now
    end
    local elapsed = now - updated_at
    if elapsed < 0 then
        elapsed = 0
    end
    cur = math.min(cur + elapsed * rate, burst)
    tokens[i] = cur
end

local allowed = true
for i = 1, n do
    if tokens[i] < 1.0 then
        allowed = false
    end
end

local reply = { allowed and 1 or 0 }
for i = 1, n do
    local final_tokens = tokens[i]
    if allowed then
        final_tokens = final_tokens - 1.0
    end
    redis.call('HSET', KEYS[i], 'tokens', tostring(final_tokens), 'updated_at', tostring(now))
    redis.call('EXPIRE', KEYS[i], ttl)
    reply[#reply + 1] = tostring(tokens[i])
end
return reply
"#;

/// Shared counter storage backed by Redis. See [`RateLimitBackend::Redis`].
///
/// A connection is established lazily on first use rather than in the
/// (synchronous, infallible) constructor. If Redis is unreachable or the
/// script invocation fails, the store fails open — it allows the request and
/// logs a warning — so a Redis outage degrades to unlimited-but-available
/// rather than taking the whole API down with false `429`s.
#[derive(Debug)]
struct RedisStore {
    client: redis::Client,
    script: redis::Script,
    conn: tokio::sync::OnceCell<redis::aio::ConnectionManager>,
}

impl RedisStore {
    fn new(url: &str) -> redis::RedisResult<Self> {
        Ok(Self {
            client: redis::Client::open(url)?,
            script: redis::Script::new(REFILL_AND_CONSUME_SCRIPT),
            conn: tokio::sync::OnceCell::new(),
        })
    }

    async fn connection(&self) -> redis::RedisResult<redis::aio::ConnectionManager> {
        self.conn
            .get_or_try_init(|| async {
                redis::aio::ConnectionManager::new(self.client.clone()).await
            })
            .await
            .cloned()
    }
}

#[async_trait::async_trait]
impl RateLimitStore for RedisStore {
    async fn refill_and_consume(
        &self,
        buckets: &[(String, BucketQuota)],
        _now: Instant,
        idle_ttl: Duration,
    ) -> RefillResult {
        let fail_open = || RefillResult {
            allowed: true,
            tokens: buckets.iter().map(|(_, q)| q.burst as f64).collect(),
        };

        let mut conn = match self.connection().await {
            Ok(conn) => conn,
            Err(err) => {
                tracing::warn!(error = %err, "rate-limit redis backend unreachable, failing open");
                return fail_open();
            }
        };

        let mut invocation = self.script.prepare_invoke();
        for (key, _) in buckets {
            invocation.key(key);
        }
        invocation.arg(buckets.len());
        invocation.arg(idle_ttl.as_secs().max(1));
        for (_, quota) in buckets {
            invocation.arg(quota.refill_per_second());
        }
        for (_, quota) in buckets {
            invocation.arg(quota.burst);
        }

        let reply: Vec<String> = match invocation.invoke_async(&mut conn).await {
            Ok(reply) => reply,
            Err(err) => {
                tracing::warn!(error = %err, "rate-limit redis script failed, failing open");
                return fail_open();
            }
        };

        let allowed = reply.first().map(|v| v == "1").unwrap_or(true);
        let tokens = reply[1..]
            .iter()
            .map(|v| v.parse::<f64>().unwrap_or(f64::MAX))
            .collect();
        RefillResult { allowed, tokens }
    }
}

#[derive(Debug, Default, Clone)]
struct Violation {
    count: u32,
    blocked_until: Option<Instant>,
    last_seen: Option<Instant>,
    scope: &'static str,
    limit: u32,
}

/// Process-local bookkeeping that intentionally stays per-instance even with
/// a shared [`RateLimitStore`]: violation backoff state and tracked-identity
/// cardinality bounding are memory-protection heuristics for this replica,
/// not part of the documented quota, so they don't need to be distributed
/// for the quota fix to be correct. `user_tiers` also stays local, matching
/// the pre-existing behavior of `set_user_tier`.
#[derive(Debug, Default)]
struct LocalState {
    cardinality_ips: HashMap<String, Instant>,
    cardinality_users: HashMap<String, Instant>,
    violations: HashMap<String, Violation>,
    user_tiers: HashMap<String, RateLimitTier>,
    checks: u64,
}

/// Cloneable middleware state. Instances are application-owned; whether
/// multiple instances (e.g. one per replica) share one enforced quota
/// depends on `RateLimitConfig::backend` — see [`RateLimitBackend`].
#[derive(Debug, Clone)]
pub struct RateLimitMiddleware {
    config: Arc<RateLimitConfig>,
    store: Arc<dyn RateLimitStore>,
    local: Arc<Mutex<LocalState>>,
}

#[derive(Debug)]
struct Decision {
    allowed: bool,
    limit: u32,
    remaining: u32,
    reset_after: Duration,
    retry_after: Duration,
    scope: &'static str,
    tier: RateLimitTier,
}

impl RateLimitMiddleware {
    pub fn new(config: RateLimitConfig) -> Self {
        for quota in [
            config.global,
            config.per_ip,
            config.free,
            config.premium,
            config.enterprise,
        ] {
            assert!(
                quota.requests_per_minute > 0,
                "rate-limit refill rate must be positive"
            );
            assert!(quota.burst > 0, "rate-limit burst must be positive");
        }
        let store: Arc<dyn RateLimitStore> = match &config.backend {
            RateLimitBackend::InProcess => Arc::new(InProcessStore::default()),
            RateLimitBackend::Redis(url) => {
                Arc::new(RedisStore::new(url).expect("invalid redis rate-limit backend URL"))
            }
        };
        Self {
            config: Arc::new(config),
            store,
            local: Arc::new(Mutex::new(LocalState::default())),
        }
    }

    /// Assign a verified user to a billing tier. Unknown users use the free tier.
    pub fn set_user_tier(&self, user_id: impl Into<String>, tier: RateLimitTier) {
        self.local
            .lock()
            .unwrap()
            .user_tiers
            .insert(user_id.into(), tier);
    }

    async fn check(&self, ip: &str, user: Option<&str>, now: Instant) -> Decision {
        let (tier, violation_key, bucket_keys, early_denial) = {
            let mut local = self.local.lock().unwrap();
            local.checks += 1;
            if local.checks % 1024 == 0 {
                let ttl = self.config.idle_ttl;
                local
                    .cardinality_ips
                    .retain(|_, t| now.saturating_duration_since(*t) < ttl);
                local
                    .cardinality_users
                    .retain(|_, t| now.saturating_duration_since(*t) < ttl);
                local.violations.retain(|_, v| {
                    v.last_seen
                        .map(|t| now.saturating_duration_since(t) < ttl)
                        .unwrap_or(false)
                });
            }

            let tier = user
                .and_then(|u| local.user_tiers.get(u).copied())
                .unwrap_or(RateLimitTier::Free);
            let user_quota = self.config.quota_for(tier);

            // Cardinality overflow keys ensure unknown identities remain limited without
            // allowing unbounded memory allocation during a distributed attack.
            let ip_key = if local.cardinality_ips.contains_key(ip)
                || local.cardinality_ips.len() < self.config.max_tracked_ips
            {
                ip.to_owned()
            } else {
                "__overflow__".to_owned()
            };
            local.cardinality_ips.insert(ip_key.clone(), now);

            let user_key = user.map(|u| {
                if local.cardinality_users.contains_key(u)
                    || local.cardinality_users.len() < self.config.max_tracked_users
                {
                    u.to_owned()
                } else {
                    "__overflow__".to_owned()
                }
            });
            if let Some(ref key) = user_key {
                local.cardinality_users.insert(key.clone(), now);
            }

            let violation_key = user_key
                .as_ref()
                .map(|u| format!("user:{u}"))
                .unwrap_or_else(|| format!("ip:{ip_key}"));

            let early_denial = if let Some(until) = local
                .violations
                .get(&violation_key)
                .and_then(|v| v.blocked_until)
            {
                if until > now {
                    // Retrying inside the advertised penalty is itself a repeated
                    // violation, so abusive tight loops rapidly reach the cap.
                    let violation = local.violations.get_mut(&violation_key).unwrap();
                    violation.count = violation.count.saturating_add(1).min(31);
                    violation.last_seen = Some(now);
                    let multiplier = 1u32 << (violation.count - 1).min(16);
                    let penalty = self
                        .config
                        .base_backoff
                        .saturating_mul(multiplier)
                        .min(self.config.max_backoff);
                    let retry = until.duration_since(now).max(penalty);
                    violation.blocked_until = Some(now + retry);
                    Some(Decision {
                        allowed: false,
                        limit: violation.limit,
                        remaining: 0,
                        reset_after: retry,
                        retry_after: retry,
                        scope: violation.scope,
                        tier,
                    })
                } else {
                    None
                }
            } else {
                None
            };

            let mut bucket_keys: Vec<(String, BucketQuota, &'static str)> = vec![
                ("global".to_owned(), self.config.global, "global"),
                (format!("ip:{ip_key}"), self.config.per_ip, "ip"),
            ];
            if let Some(ref key) = user_key {
                bucket_keys.push((format!("user:{key}"), user_quota, "user"));
            }

            (tier, violation_key, bucket_keys, early_denial)
        };

        if let Some(decision) = early_denial {
            return decision;
        }

        let store_buckets: Vec<(String, BucketQuota)> = bucket_keys
            .iter()
            .map(|(key, quota, _)| (key.clone(), *quota))
            .collect();
        let result = self
            .store
            .refill_and_consume(&store_buckets, now, self.config.idle_ttl)
            .await;

        let mut candidates: Vec<(&'static str, BucketQuota, f64)> = bucket_keys
            .iter()
            .zip(result.tokens.iter())
            .map(|((_, quota, scope), tokens)| (*scope, *quota, *tokens))
            .collect();
        // Report the bucket with the least proportional quota remaining.
        candidates.sort_by(|a, b| {
            (a.2 / a.1.burst as f64)
                .partial_cmp(&(b.2 / b.1.burst as f64))
                .unwrap()
                .then_with(|| a.1.burst.cmp(&b.1.burst))
        });
        let (scope, quota, tokens) = candidates[0];

        if !result.allowed {
            let (failed_scope, failed_quota, token_wait) = candidates
                .iter()
                .filter(|(_, _, t)| *t < 1.0)
                .map(|(scope, quota, tokens)| {
                    let wait = Duration::from_secs_f64(
                        ((1.0 - tokens) / quota.refill_per_second()).max(0.0),
                    );
                    (*scope, *quota, wait)
                })
                .max_by_key(|(_, _, wait)| *wait)
                .expect("store reported denial without an exhausted bucket");

            let mut local = self.local.lock().unwrap();
            let violation = local.violations.entry(violation_key).or_default();
            violation.count = violation.count.saturating_add(1).min(31);
            violation.last_seen = Some(now);
            violation.scope = failed_scope;
            violation.limit = failed_quota.burst;
            let multiplier = 1u32 << (violation.count - 1).min(16);
            let backoff = self
                .config
                .base_backoff
                .saturating_mul(multiplier)
                .min(self.config.max_backoff);
            let retry = token_wait.max(backoff);
            violation.blocked_until = Some(now + retry);
            return Decision {
                allowed: false,
                limit: failed_quota.burst,
                remaining: 0,
                reset_after: token_wait,
                retry_after: retry,
                scope: failed_scope,
                tier,
            };
        }

        self.local.lock().unwrap().violations.remove(&violation_key);
        let remaining = tokens.floor().max(1.0) as u32 - 1;
        let reset_after = Duration::from_secs_f64(
            ((quota.burst as f64 - (tokens - 1.0)) / quota.refill_per_second()).max(0.0),
        );
        Decision {
            allowed: true,
            limit: quota.burst,
            remaining,
            reset_after,
            retry_after: Duration::ZERO,
            scope,
            tier,
        }
    }
}

fn seconds_ceil(duration: Duration) -> u64 {
    duration
        .as_secs()
        .saturating_add(u64::from(duration.subsec_nanos() > 0))
}

fn add_headers(response: &mut Response, decision: &Decision) {
    let reset = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_add(seconds_ceil(decision.reset_after));
    for (name, value) in [
        ("x-ratelimit-limit", decision.limit.to_string()),
        ("x-ratelimit-remaining", decision.remaining.to_string()),
        ("x-ratelimit-reset", reset.to_string()),
        ("x-ratelimit-scope", decision.scope.to_string()),
        ("x-ratelimit-tier", decision.tier.as_str().to_string()),
    ] {
        response.headers_mut().insert(
            HeaderName::from_static(name),
            HeaderValue::from_str(&value).unwrap(),
        );
    }
    if !decision.allowed {
        response.headers_mut().insert(
            "retry-after",
            HeaderValue::from_str(&seconds_ceil(decision.retry_after).max(1).to_string()).unwrap(),
        );
    }
}

fn parse_forwarded_ip(req: &Request) -> Option<IpAddr> {
    req.headers()
        .get("x-forwarded-for")?
        .to_str()
        .ok()?
        .split(',')
        .next()?
        .trim()
        .parse()
        .ok()
}

/// Axum middleware entry point for `from_fn_with_state`.
pub async fn rate_limit_middleware(
    State(limiter): State<RateLimitMiddleware>,
    req: Request,
    next: Next,
) -> Response {
    let peer_ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip());
    let ip = if limiter.config.trust_proxy_headers {
        parse_forwarded_ip(&req).or(peer_ip)
    } else {
        peer_ip
    }
    .map(|v| v.to_string())
    .unwrap_or_else(|| "unknown".to_string());
    let user = req
        .extensions()
        .get::<AuthExtension>()
        .map(|a| a.0.sub.as_str())
        .or_else(|| {
            req.headers()
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
                .filter(|key| key.len() <= 256)
        });
    let decision = limiter.check(&ip, user, Instant::now()).await;
    let mut response = if decision.allowed {
        next.run(req).await
    } else {
        (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "rate_limit_exceeded",
                "scope": decision.scope,
                "retry_after_seconds": seconds_ceil(decision.retry_after).max(1),
            })),
        )
            .into_response()
    };
    add_headers(&mut response, &decision);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, middleware, routing::get, Router};
    use std::sync::Barrier;
    use tower::ServiceExt;

    fn config() -> RateLimitConfig {
        RateLimitConfig {
            global: BucketQuota::new(600, 100),
            per_ip: BucketQuota::new(60, 5),
            free: BucketQuota::new(60, 3),
            premium: BucketQuota::new(600, 10),
            enterprise: BucketQuota::new(6_000, 50),
            base_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(8),
            ..RateLimitConfig::default()
        }
    }

    #[tokio::test]
    async fn burst_is_limited_and_tokens_recover() {
        let limiter = RateLimitMiddleware::new(config());
        let start = Instant::now();
        for _ in 0..3 {
            assert!(limiter.check("1.2.3.4", Some("free"), start).await.allowed);
        }
        let denied = limiter.check("1.2.3.4", Some("free"), start).await;
        assert!(!denied.allowed);
        assert_eq!(denied.scope, "user");
        assert!(
            limiter
                .check("1.2.3.4", Some("free"), start + Duration::from_secs(2))
                .await
                .allowed
        );
    }

    #[tokio::test]
    async fn tiers_and_users_are_isolated() {
        let limiter = RateLimitMiddleware::new(config());
        limiter.set_user_tier("paid", RateLimitTier::Premium);
        let now = Instant::now();
        for _ in 0..5 {
            assert!(limiter.check("10.0.0.1", Some("paid"), now).await.allowed);
        }
        assert!(!limiter.check("10.0.0.1", Some("paid"), now).await.allowed); // IP bucket
        assert!(limiter.check("10.0.0.2", Some("other"), now).await.allowed);
    }

    #[tokio::test]
    async fn repeated_violations_back_off_exponentially() {
        let limiter = RateLimitMiddleware::new(config());
        let start = Instant::now();
        for _ in 0..3 {
            limiter.check("1.1.1.1", Some("u"), start).await;
        }
        let first = limiter.check("1.1.1.1", Some("u"), start).await;
        let second = limiter.check("1.1.1.1", Some("u"), start).await;
        assert!(second.retry_after > first.retry_after);
    }

    #[test]
    fn concurrent_requests_cannot_overspend_bucket() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let limiter = Arc::new(RateLimitMiddleware::new(config()));
        let barrier = Arc::new(Barrier::new(20));
        let now = Instant::now();
        let handles: Vec<_> = (0..20)
            .map(|_| {
                let limiter = limiter.clone();
                let barrier = barrier.clone();
                let handle = rt.handle().clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    handle
                        .block_on(limiter.check("2.2.2.2", Some("same"), now))
                        .allowed
                })
            })
            .collect();
        let allowed = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|allowed| *allowed)
            .count();
        assert_eq!(allowed, 3);
    }

    #[tokio::test]
    async fn zero_tracking_capacity_uses_bounded_overflow_bucket() {
        let mut cfg = config();
        cfg.max_tracked_ips = 0;
        cfg.max_tracked_users = 0;
        let limiter = RateLimitMiddleware::new(cfg);
        let now = Instant::now();
        assert!(limiter.check("a", Some("a"), now).await.allowed);
        assert!(limiter.check("b", Some("b"), now).await.allowed);
        assert!(limiter.check("c", Some("c"), now).await.allowed);
        assert!(!limiter.check("d", Some("d"), now).await.allowed);
        let local = limiter.local.lock().unwrap();
        assert_eq!(local.cardinality_ips.len(), 1);
        assert_eq!(local.cardinality_users.len(), 1);
    }

    #[tokio::test]
    async fn middleware_returns_client_headers_and_429_body() {
        let mut cfg = config();
        cfg.per_ip = BucketQuota::new(60, 1);
        let app = Router::new()
            .route("/", get(|| async { StatusCode::NO_CONTENT }))
            .layer(middleware::from_fn_with_state(
                RateLimitMiddleware::new(cfg),
                rate_limit_middleware,
            ));
        let request = || {
            let mut request = Request::builder().uri("/").body(Body::empty()).unwrap();
            request
                .extensions_mut()
                .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 1234))));
            request
        };

        let first = app.clone().oneshot(request()).await.unwrap();
        assert_eq!(first.status(), StatusCode::NO_CONTENT);
        assert_eq!(first.headers()["x-ratelimit-remaining"], "0");
        let denied = app.oneshot(request()).await.unwrap();
        assert_eq!(denied.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(denied.headers().contains_key("retry-after"));
        assert_eq!(denied.headers()["x-ratelimit-scope"], "ip");
        let body = axum::body::to_bytes(denied.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap()["error"],
            "rate_limit_exceeded"
        );
    }

    /// Two independent `RateLimitMiddleware` instances (standing in for two
    /// replicas behind a load balancer) pointed at the same Redis backend
    /// must enforce one combined quota for a given client, not two
    /// independent ones. This is the behavior #787 reports as broken for the
    /// in-process store.
    #[tokio::test]
    async fn redis_backend_enforces_one_combined_quota_across_instances() {
        use testcontainers::runners::AsyncRunner;
        use testcontainers_modules::redis::Redis;

        let container = Redis::default()
            .start()
            .await
            .expect("failed to start redis container - is Docker available?");
        let port = container
            .get_host_port_ipv4(6379)
            .await
            .expect("failed to map redis port");
        let url = format!("redis://127.0.0.1:{port}");

        let mut cfg = config();
        cfg.backend = RateLimitBackend::Redis(url.clone());
        let replica_a = RateLimitMiddleware::new(cfg.clone());
        cfg.backend = RateLimitBackend::Redis(url);
        let replica_b = RateLimitMiddleware::new(cfg);

        // Free-tier burst is 3. Six requests split evenly across two
        // "replicas" for the same client must still only allow 3 total.
        let now = Instant::now();
        let mut allowed = 0;
        for i in 0..6 {
            let replica = if i % 2 == 0 { &replica_a } else { &replica_b };
            if replica
                .check("9.9.9.9", Some("shared-client"), now)
                .await
                .allowed
            {
                allowed += 1;
            }
        }
        assert_eq!(
            allowed, 3,
            "combined quota across both replicas must equal one bucket's capacity, not 2x"
        );
    }

    /// The in-process backend, by contrast, does not share state: two
    /// instances really do enforce independent quotas. This documents that
    /// gap (matching the updated docs/api-reference.md caveat) rather than
    /// leaving it as an untested assumption.
    #[tokio::test]
    async fn in_process_backend_does_not_share_quota_across_instances() {
        let cfg = config();
        let replica_a = RateLimitMiddleware::new(cfg.clone());
        let replica_b = RateLimitMiddleware::new(cfg);
        let now = Instant::now();

        for _ in 0..3 {
            assert!(
                replica_a
                    .check("8.8.8.8", Some("client"), now)
                    .await
                    .allowed
            );
        }
        assert!(
            replica_b
                .check("8.8.8.8", Some("client"), now)
                .await
                .allowed,
            "independent in-process stores each grant their own full quota"
        );
    }
}
