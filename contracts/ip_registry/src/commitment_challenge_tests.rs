#[cfg(test)]
mod commitment_challenge_tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, Bytes, BytesN, Env,
    };

    use crate::{IpRegistry, IpRegistryClient, DEFAULT_CHALLENGE_TTL_SECONDS};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn setup() -> (Env, IpRegistryClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &contract_id);
        (env, registry)
    }

    fn commitment(env: &Env, seed: &[u8]) -> BytesN<32> {
        let data = Bytes::from_slice(env, seed);
        env.crypto().sha256(&data).into()
    }

    /// Mirrors the contract's expected response: sha256(commitment_hash || challenge).
    fn expected_response(env: &Env, commitment_hash: &BytesN<32>, challenge: &Bytes) -> BytesN<32> {
        let mut preimage = Bytes::new(env);
        preimage.append(&commitment_hash.clone().into());
        preimage.append(challenge);
        env.crypto().sha256(&preimage).into()
    }

    // ── Challenge generation ──────────────────────────────────────────────────

    #[test]
    fn challenge_is_32_random_bytes() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &commitment(&env, b"c1"), &0);

        let first = registry.challenge_ownership(&ip_id);
        assert_eq!(first.len(), 32);

        let second = registry.challenge_ownership(&ip_id);
        assert_ne!(first, second, "each issued challenge must be fresh");
    }

    #[test]
    fn issuing_counts_toward_stats() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &commitment(&env, b"c2"), &0);

        registry.challenge_ownership(&ip_id);
        registry.challenge_ownership(&ip_id);

        let stats = registry.get_challenge_stats(&ip_id);
        assert_eq!(stats.issued, 2);
        assert_eq!(stats.succeeded, 0);
        assert_eq!(stats.failed, 0);
    }

    #[test]
    #[should_panic]
    fn challenge_for_unknown_commitment_panics() {
        let (_env, registry) = setup();
        registry.challenge_ownership(&999);
    }

    #[test]
    #[should_panic]
    fn challenge_for_revoked_commitment_panics() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &commitment(&env, b"c3"), &0);
        registry.revoke_ip(&ip_id);
        registry.challenge_ownership(&ip_id);
    }

    // ── Response verification ─────────────────────────────────────────────────

    #[test]
    fn correct_response_succeeds_and_is_counted() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let hash = commitment(&env, b"c4");
        let ip_id = registry.commit_ip(&owner, &hash, &0);

        let challenge = registry.challenge_ownership(&ip_id);
        let response = expected_response(&env, &hash, &challenge);

        assert!(registry.respond_to_commitment_challenge(&ip_id, &response));

        let stats = registry.get_challenge_stats(&ip_id);
        assert_eq!(stats.succeeded, 1);
        assert_eq!(stats.failed, 0);
    }

    #[test]
    fn wrong_response_fails_without_reverting_and_is_counted() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &commitment(&env, b"c5"), &0);
        registry.challenge_ownership(&ip_id);

        let wrong = BytesN::from_array(&env, &[0u8; 32]);
        assert!(!registry.respond_to_commitment_challenge(&ip_id, &wrong));

        let stats = registry.get_challenge_stats(&ip_id);
        assert_eq!(stats.failed, 1, "failure must persist, not revert");
        assert_eq!(stats.succeeded, 0);
    }

    #[test]
    #[should_panic]
    fn challenge_is_single_use_after_wrong_answer() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &commitment(&env, b"c6"), &0);
        registry.challenge_ownership(&ip_id);

        let wrong = BytesN::from_array(&env, &[0u8; 32]);
        registry.respond_to_commitment_challenge(&ip_id, &wrong);
        // The challenge was consumed by the first answer; a retry must panic.
        registry.respond_to_commitment_challenge(&ip_id, &wrong);
    }

    #[test]
    #[should_panic]
    fn respond_without_open_challenge_panics() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &commitment(&env, b"c7"), &0);
        let wrong = BytesN::from_array(&env, &[0u8; 32]);
        registry.respond_to_commitment_challenge(&ip_id, &wrong);
    }

    #[test]
    #[should_panic]
    fn respond_after_expiry_panics() {
        let (env, registry) = setup();
        env.ledger().with_mut(|li| li.timestamp = 1_000);
        let owner = Address::generate(&env);
        let hash = commitment(&env, b"c8");
        let ip_id = registry.commit_ip(&owner, &hash, &0);
        let challenge = registry.challenge_ownership(&ip_id);
        let response = expected_response(&env, &hash, &challenge);

        env.ledger()
            .with_mut(|li| li.timestamp = 1_000 + DEFAULT_CHALLENGE_TTL_SECONDS + 1);
        registry.respond_to_commitment_challenge(&ip_id, &response);
    }

    #[test]
    fn reissuing_replaces_unanswered_challenge() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let hash = commitment(&env, b"c9");
        let ip_id = registry.commit_ip(&owner, &hash, &0);

        let stale = registry.challenge_ownership(&ip_id);
        let fresh = registry.challenge_ownership(&ip_id);
        assert_ne!(stale, fresh);

        // Only the most recent challenge is valid.
        let response = expected_response(&env, &hash, &fresh);
        assert!(registry.respond_to_commitment_challenge(&ip_id, &response));
    }

    // ── Success rate ──────────────────────────────────────────────────────────

    #[test]
    fn success_rate_is_zero_before_any_answer() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &commitment(&env, b"c10"), &0);
        registry.challenge_ownership(&ip_id);
        assert_eq!(registry.get_challenge_success_rate_bps(&ip_id), 0);
    }

    #[test]
    fn success_rate_reflects_answered_challenges() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let hash = commitment(&env, b"c11");
        let ip_id = registry.commit_ip(&owner, &hash, &0);

        // One success.
        let challenge = registry.challenge_ownership(&ip_id);
        let response = expected_response(&env, &hash, &challenge);
        assert!(registry.respond_to_commitment_challenge(&ip_id, &response));

        // One failure.
        registry.challenge_ownership(&ip_id);
        let wrong = BytesN::from_array(&env, &[0u8; 32]);
        assert!(!registry.respond_to_commitment_challenge(&ip_id, &wrong));

        assert_eq!(registry.get_challenge_success_rate_bps(&ip_id), 5_000);
    }
}
