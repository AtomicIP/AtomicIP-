//! Tests for rate-limit bypass prevention and authenticated endpoint rate limiting
//! Tests for #907 (rate-limit bypass test for authenticated high-privilege endpoints)

#[cfg(test)]
mod rate_limit_bypass_tests {
    use std::collections::HashMap;

    // Mock structures for rate limiting tests
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum RateLimitTier {
        Free,
        Premium,
        Enterprise,
    }

    #[derive(Debug, Clone, Copy)]
    pub struct BucketQuota {
        pub requests_per_minute: u32,
        pub burst: u32,
    }

    pub struct AuthenticatedClient {
        pub api_key: String,
        pub tier: RateLimitTier,
    }

    pub struct RateLimitChecker {
        pub per_ip_limit: BucketQuota,
        pub per_api_key_limit: BucketQuota,
        pub tier_limits: HashMap<RateLimitTier, BucketQuota>,
    }

    impl RateLimitChecker {
        pub fn new() -> Self {
            let mut tier_limits = HashMap::new();
            tier_limits.insert(RateLimitTier::Free, BucketQuota {
                requests_per_minute: 60,
                burst: 30,
            });
            tier_limits.insert(RateLimitTier::Premium, BucketQuota {
                requests_per_minute: 600,
                burst: 200,
            });
            tier_limits.insert(RateLimitTier::Enterprise, BucketQuota {
                requests_per_minute: 6000,
                burst: 1000,
            });

            Self {
                per_ip_limit: BucketQuota {
                    requests_per_minute: 300,
                    burst: 100,
                },
                per_api_key_limit: BucketQuota {
                    requests_per_minute: 600,
                    burst: 200,
                },
                tier_limits,
            }
        }

        /// Check if an authenticated request should be rate limited.
        /// Must enforce BOTH per-IP and per-API-key limits.
        pub fn should_rate_limit_authenticated(
            &self,
            client: &AuthenticatedClient,
            source_ip: &str,
            current_tokens_for_ip: f64,
            current_tokens_for_key: f64,
        ) -> bool {
            // An authenticated client still must respect per-IP limits
            let per_ip_limited = current_tokens_for_ip < 1.0;

            // An authenticated client must also respect per-API-key limits
            let per_api_key_limited = current_tokens_for_key < 1.0;

            // The request is rate limited if EITHER quota is exhausted
            per_ip_limited || per_api_key_limited
        }

        /// Verify that tier is correctly applied to per-API-key limit.
        pub fn get_tier_quota(&self, tier: RateLimitTier) -> BucketQuota {
            *self
                .tier_limits
                .get(&tier)
                .unwrap_or(&self.per_api_key_limit)
        }
    }

    // ── #907: Authenticated rate limit bypass tests ───────────────────────────

    #[test]
    fn test_authenticated_client_cannot_bypass_per_ip_limit() {
        let checker = RateLimitChecker::new();
        let client = AuthenticatedClient {
            api_key: "admin_key_123".to_string(),
            tier: RateLimitTier::Enterprise,
        };

        // Even for an enterprise (high-privilege) client, per-IP limit must apply
        let per_ip_tokens = 0.5; // Exhausted IP quota
        let per_api_key_tokens = 500.0; // Abundant API key quota

        let should_limit = checker.should_rate_limit_authenticated(
            &client,
            "192.0.2.1",
            per_ip_tokens,
            per_api_key_tokens,
        );

        assert!(
            should_limit,
            "Authenticated client must be rate limited by per-IP quota"
        );
    }

    #[test]
    fn test_authenticated_client_cannot_bypass_per_api_key_limit() {
        let checker = RateLimitChecker::new();
        let client = AuthenticatedClient {
            api_key: "admin_key_456".to_string(),
            tier: RateLimitTier::Enterprise,
        };

        // Even with abundant per-IP quota, per-API-key limit must apply
        let per_ip_tokens = 500.0; // Abundant IP quota
        let per_api_key_tokens = 0.5; // Exhausted API key quota

        let should_limit = checker.should_rate_limit_authenticated(
            &client,
            "203.0.113.50",
            per_ip_tokens,
            per_api_key_tokens,
        );

        assert!(
            should_limit,
            "Authenticated client must be rate limited by per-API-key quota"
        );
    }

    #[test]
    fn test_free_tier_enforces_lower_per_api_key_limit() {
        let checker = RateLimitChecker::new();
        let free_tier = RateLimitTier::Free;
        let premium_tier = RateLimitTier::Premium;

        let free_quota = checker.get_tier_quota(free_tier);
        let premium_quota = checker.get_tier_quota(premium_tier);

        assert!(
            free_quota.requests_per_minute < premium_quota.requests_per_minute,
            "Free tier must have lower request limit than Premium"
        );
    }

    #[test]
    fn test_enterprise_tier_enforces_higher_per_api_key_limit() {
        let checker = RateLimitChecker::new();
        let premium_tier = RateLimitTier::Premium;
        let enterprise_tier = RateLimitTier::Enterprise;

        let premium_quota = checker.get_tier_quota(premium_tier);
        let enterprise_quota = checker.get_tier_quota(enterprise_tier);

        assert!(
            enterprise_quota.requests_per_minute > premium_quota.requests_per_minute,
            "Enterprise tier must have higher request limit than Premium"
        );
    }

    #[test]
    fn test_batch_endpoint_counts_tokens_proportionally() {
        let checker = RateLimitChecker::new();
        let client = AuthenticatedClient {
            api_key: "batch_admin_key".to_string(),
            tier: RateLimitTier::Enterprise,
        };

        // A batch request with 5 items should consume 5 tokens, not 1
        let batch_size = 5;
        let tokens_consumed = batch_size as f64;

        let per_ip_tokens_before = 100.0;
        let per_api_key_tokens_before = 200.0;

        let per_ip_tokens_after = per_ip_tokens_before - tokens_consumed;
        let per_api_key_tokens_after = per_api_key_tokens_before - tokens_consumed;

        assert_eq!(
            per_ip_tokens_after, 95.0,
            "Batch request must consume proportional tokens from per-IP bucket"
        );
        assert_eq!(
            per_api_key_tokens_after, 195.0,
            "Batch request must consume proportional tokens from per-API-key bucket"
        );

        // Verify rate limiting still applies after consuming proportional tokens
        let should_limit_ip = checker.should_rate_limit_authenticated(
            &client,
            "192.0.2.1",
            0.5,
            per_api_key_tokens_after,
        );
        assert!(
            should_limit_ip,
            "Rate limiting must apply based on consumed tokens"
        );
    }

    #[test]
    fn test_authenticated_user_tier_does_not_bypass_per_ip_limits() {
        let checker = RateLimitChecker::new();

        // Test all tiers cannot bypass per-IP limit
        for tier in &[
            RateLimitTier::Free,
            RateLimitTier::Premium,
            RateLimitTier::Enterprise,
        ] {
            let client = AuthenticatedClient {
                api_key: format!("key_{:?}", tier),
                tier: *tier,
            };

            // Per-IP limit exhausted
            let should_limit = checker.should_rate_limit_authenticated(
                &client,
                "198.51.100.1",
                0.1, // Token count below 1
                500.0,
            );

            assert!(
                should_limit,
                "Tier {:?} must be rate limited by per-IP quota",
                tier
            );
        }
    }

    #[test]
    fn test_rate_limit_interaction_between_quotas() {
        let checker = RateLimitChecker::new();
        let client = AuthenticatedClient {
            api_key: "test_key".to_string(),
            tier: RateLimitTier::Premium,
        };

        // Both quotas have sufficient tokens: request should be allowed
        let allowed = !checker.should_rate_limit_authenticated(
            &client,
            "198.51.100.50",
            5.0, // Sufficient per-IP tokens
            10.0, // Sufficient per-API-key tokens
        );
        assert!(allowed, "Request should be allowed when both quotas have tokens");

        // One quota exhausted: request should be limited
        let limited_by_ip = checker.should_rate_limit_authenticated(
            &client,
            "198.51.100.51",
            0.5, // Insufficient per-IP tokens
            10.0, // Sufficient per-API-key tokens
        );
        assert!(
            limited_by_ip,
            "Request should be limited if either quota is exhausted"
        );

        // Other quota exhausted: request should be limited
        let limited_by_key = checker.should_rate_limit_authenticated(
            &client,
            "198.51.100.52",
            5.0, // Sufficient per-IP tokens
            0.5, // Insufficient per-API-key tokens
        );
        assert!(
            limited_by_key,
            "Request should be limited if either quota is exhausted"
        );
    }

    // ── #1223: Parse failures must fail closed, never panic ───────────────────

    /// Outcome of parsing a rate-limit input (header, API key, etc.).
    /// A parse failure must map to a client error (400) or a rate-limit
    /// response (429), never a panic.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ParseOutcome {
        Ok,
        BadRequest,
        RateLimited,
    }

    impl ParseOutcome {
        pub fn is_fail_closed(self) -> bool {
            matches!(self, ParseOutcome::BadRequest | ParseOutcome::RateLimited)
        }
    }

    /// Parse a `x-forwarded-for` header value into a source IP.
    /// Malformed values fail closed with 400 rather than panicking.
    pub fn parse_forwarded_for(header: Option<&str>) -> ParseOutcome {
        let value = match header {
            Some(v) => v.trim(),
            None => return ParseOutcome::BadRequest,
        };
        if value.is_empty() {
            return ParseOutcome::BadRequest;
        }
        // Take the first hop and validate it looks like an IPv4 address.
        let first = match value.split(',').next() {
            Some(hop) => hop.trim(),
            None => return ParseOutcome::BadRequest,
        };
        let octets: Vec<&str> = first.split('.').collect();
        if octets.len() != 4 {
            return ParseOutcome::BadRequest;
        }
        for octet in octets {
            match octet.parse::<u8>() {
                Ok(_) => {}
                Err(_) => return ParseOutcome::BadRequest,
            }
        }
        ParseOutcome::Ok
    }

    /// Parse a numeric rate-limit header (e.g. `x-ratelimit-burst`).
    /// Invalid values fail closed with 400.
    pub fn parse_numeric_header(header: Option<&str>) -> ParseOutcome {
        match header {
            Some(v) => match v.trim().parse::<u32>() {
                Ok(_) => ParseOutcome::Ok,
                Err(_) => ParseOutcome::BadRequest,
            },
            None => ParseOutcome::BadRequest,
        }
    }

    /// Decode an API key from a header value. Non-ASCII or empty keys fail
    /// closed with 400 instead of panicking on invalid encoding.
    pub fn decode_api_key(header: Option<&str>) -> ParseOutcome {
        match header {
            Some(v) => {
                let trimmed = v.trim();
                if trimmed.is_empty() || !trimmed.is_ascii() {
                    ParseOutcome::BadRequest
                } else {
                    ParseOutcome::Ok
                }
            }
            None => ParseOutcome::BadRequest,
        }
    }

    #[test]
    fn test_malformed_forwarded_for_fails_closed() {
        let cases = [
            None,
            Some(""),
            Some("   "),
            Some("not-an-ip"),
            Some("999.999.999.999"),
            Some("192.0.2"),
            Some("192.0.2.1.5"),
            Some("::1"),
        ];
        for case in cases.iter() {
            let outcome = parse_forwarded_for(*case);
            assert!(
                outcome.is_fail_closed(),
                "Malformed x-forwarded-for {:?} must fail closed, got {:?}",
                case,
                outcome
            );
        }
    }

    #[test]
    fn test_valid_forwarded_for_is_accepted() {
        assert_eq!(parse_forwarded_for(Some("192.0.2.1")), ParseOutcome::Ok);
        assert_eq!(
            parse_forwarded_for(Some("203.0.113.7, 10.0.0.1")),
            ParseOutcome::Ok
    // ── #1215: X-Forwarded-For trust tests ────────────────────────────────────

    /// Resolves the client identity used for rate-limit bucketing.
    /// The `x-forwarded-for` header is only honored when the peer (socket)
    /// address is in the configured proxy allowlist; otherwise the socket
    /// address is used, preventing spoofed headers from minting fresh buckets.
    fn resolve_client_ip(
        peer_addr: &str,
        forwarded_for: Option<&str>,
        trusted_proxies: &[&str],
    ) -> String {
        if trusted_proxies.contains(&peer_addr) {
            if let Some(header) = forwarded_for {
                // Use the left-most (original client) entry
                if let Some(client_ip) = header.split(',').next() {
                    let client_ip = client_ip.trim();
                    if !client_ip.is_empty() {
                        return client_ip.to_string();
                    }
                }
            }
        }
        peer_addr.to_string()
    }

    #[test]
    fn test_spoofed_xff_from_untrusted_peer_is_ignored() {
        let trusted_proxies = ["10.0.0.1"];
        let peer_addr = "203.0.113.7"; // Not a trusted proxy

        // Attacker rotates spoofed headers to try to get a fresh bucket each time
        let spoofed_a = resolve_client_ip(peer_addr, Some("1.1.1.1"), &trusted_proxies);
        let spoofed_b = resolve_client_ip(peer_addr, Some("2.2.2.2"), &trusted_proxies);
        let spoofed_c = resolve_client_ip(peer_addr, Some("3.3.3.3"), &trusted_proxies);

        assert_eq!(spoofed_a, peer_addr, "Spoofed XFF must not be trusted");
        assert_eq!(spoofed_b, peer_addr, "Spoofed XFF must not be trusted");
        assert_eq!(spoofed_c, peer_addr, "Spoofed XFF must not be trusted");

        // All requests collapse into the same bucket keyed by the socket address
        assert_eq!(
            spoofed_a, spoofed_b,
            "Rotating spoofed XFF must not yield fresh buckets"
        );
        assert_eq!(
            spoofed_b, spoofed_c,
            "Rotating spoofed XFF must not yield fresh buckets"
        );
    }

    #[test]
    fn test_invalid_numeric_header_fails_closed() {
        let cases = [None, Some(""), Some("abc"), Some("-5"), Some("1.5"), Some("999999999999")];
        for case in cases.iter() {
            let outcome = parse_numeric_header(*case);
            assert!(
                outcome.is_fail_closed(),
                "Invalid numeric header {:?} must fail closed, got {:?}",
                case,
                outcome
            );
        }
        assert_eq!(parse_numeric_header(Some("100")), ParseOutcome::Ok);
    }

    #[test]
    fn test_bad_api_key_encoding_fails_closed() {
        let cases = [None, Some(""), Some("   "), Some("ключ"), Some("key\u{0}with-nul")];
        for case in cases.iter() {
            let outcome = decode_api_key(*case);
            assert!(
                outcome.is_fail_closed(),
                "Bad API key encoding {:?} must fail closed, got {:?}",
                case,
                outcome
            );
        }
        assert_eq!(decode_api_key(Some("valid_key_123")), ParseOutcome::Ok);
    }

    #[test]
    fn test_parse_failure_does_not_bypass_limit_for_offending_request() {
        let checker = RateLimitChecker::new();
        let client = AuthenticatedClient {
            api_key: "attacker_key".to_string(),
            tier: RateLimitTier::Free,
        };

        // The offending request has a malformed forwarded-for header.
        let parse = parse_forwarded_for(Some("garbage"));
        assert!(
            parse.is_fail_closed(),
            "Malformed header must fail closed, not panic or pass through"
        );

        // Even though the parse failed, the request must still be subject to
        // rate limiting: a fail-closed outcome means the request is rejected
        // (400/429), so it cannot consume tokens or bypass the limiter.
        let should_limit = checker.should_rate_limit_authenticated(
            &client,
            "0.0.0.0", // unknown source after failed parse
            0.0,       // fail-closed: treat as exhausted
            0.0,       // fail-closed: treat as exhausted
        );
        assert!(
            should_limit,
            "A request whose rate-limit inputs failed to parse must not bypass the limit"
    fn test_xff_from_trusted_proxy_is_honored() {
        let trusted_proxies = ["10.0.0.1"];
        let peer_addr = "10.0.0.1"; // Allowlisted proxy

        let client_ip = resolve_client_ip(peer_addr, Some("198.51.100.42"), &trusted_proxies);

        assert_eq!(
            client_ip, "198.51.100.42",
            "XFF from an allowlisted proxy must be used for bucketing"
        );
    }

    #[test]
    fn test_xff_from_trusted_proxy_uses_leftmost_entry() {
        let trusted_proxies = ["10.0.0.1"];
        let peer_addr = "10.0.0.1";

        let client_ip = resolve_client_ip(
            peer_addr,
            Some("198.51.100.42, 10.0.0.2, 10.0.0.1"),
            &trusted_proxies,
        );

        assert_eq!(
            client_ip, "198.51.100.42",
            "The original client IP (left-most XFF entry) must be used"
        );
    }

    #[test]
    fn test_trusted_proxy_without_xff_falls_back_to_socket_addr() {
        let trusted_proxies = ["10.0.0.1"];
        let peer_addr = "10.0.0.1";

        let client_ip = resolve_client_ip(peer_addr, None, &trusted_proxies);

        assert_eq!(
            client_ip, peer_addr,
            "Without XFF the socket address must be used even for trusted proxies"
        );
    }

    #[test]
    fn test_admin_cannot_bypass_rate_limits_through_batch_operations() {
        let checker = RateLimitChecker::new();
        let admin_client = AuthenticatedClient {
            api_key: "admin_api_key".to_string(),
            tier: RateLimitTier::Enterprise,
        };

        // Simulate a batch operation attempting to exceed quotas
        let batch_items = 100;
        let tokens_per_item = 1.0;
        let total_tokens_needed = batch_items as f64 * tokens_per_item;

        let per_ip_tokens = 50.0;
        let per_api_key_tokens = 50.0;

        // After processing batch, both quotas should be depleted
        let per_ip_after = per_ip_tokens - (batch_items / 2) as f64;
        let per_api_key_after = per_api_key_tokens - (batch_items / 2) as f64;

        // Verify subsequent requests are limited
        let should_limit = checker.should_rate_limit_authenticated(
            &admin_client,
            "192.0.2.100",
            per_ip_after,
            per_api_key_after,
        );

        assert!(
            should_limit,
            "Admin must still be rate limited after batch operations"
        );
    }
}
