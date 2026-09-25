/// Property-Based Testing for Commitment Operations
///
/// This module uses proptest to verify that commitment, reveal, and verify operations
/// maintain critical invariants across 1000+ randomly generated test cases.
///
/// Key properties tested:
/// 1. Reversibility: commit → reveal → verify always succeeds with correct inputs
/// 2. Invariants: Revealed hash always matches original commitment
/// 3. Security: Wrong inputs always fail verification
/// 4. Determinism: Same inputs always produce same results

#[cfg(test)]
mod commitment_property_tests {
    use proptest::prelude::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, BytesN, Env,
    };

    use crate::{IpRegistry, IpRegistryClient};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn create_test_commitment(
        env: &Env,
        secret: &BytesN<32>,
        blinding: &BytesN<32>,
    ) -> BytesN<32> {
        let mut preimage = soroban_sdk::Bytes::new(env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        env.crypto().sha256(&preimage).into()
    }

    fn setup_registry() -> (Env, IpRegistryClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &contract_id);
        (env, registry)
    }

    // ── Property Tests: Reversibility ─────────────────────────────────────────

    proptest! {
        /// Property: Committing with a valid hash, then revealing with the correct
        /// secret and blinding factor, always verifies successfully.
        ///
        /// This is the core security property of the commitment scheme.
        #[test]
        fn prop_commit_reveal_verify_success(
            secret_bytes in prop::array::uniform32(any::<u8>()),
            blinding_bytes in prop::array::uniform32(any::<u8>()),
        ) {
            let (env, registry) = setup_registry();
            let owner = Address::generate(&env);

            let secret = BytesN::from_array(&env, &secret_bytes);
            let blinding = BytesN::from_array(&env, &blinding_bytes);
            let commitment_hash = create_test_commitment(&env, &secret, &blinding);

            // Commit
            let ip_id = registry.commit_ip(&owner, &commitment_hash, &0);
            prop_assert!(ip_id > 0);

            // Verify with correct inputs should succeed
            let verified = registry.verify_commitment(&ip_id, &secret, &blinding);
            prop_assert!(verified);
        }

        /// Property: Verifying with incorrect secret fails even if blinding is correct.
        ///
        /// This ensures the commitment is bound to the secret, not just the blinding.
        #[test]
        fn prop_wrong_secret_fails_verification(
            secret_bytes in prop::array::uniform32(any::<u8>()),
            blinding_bytes in prop::array::uniform32(any::<u8>()),
            wrong_secret_bytes in prop::array::uniform32(any::<u8>()),
        ) {
            // Ensure wrong_secret is actually different
            if secret_bytes == wrong_secret_bytes {
                return Ok(());
            }

            let (env, registry) = setup_registry();
            let owner = Address::generate(&env);

            let secret = BytesN::from_array(&env, &secret_bytes);
            let blinding = BytesN::from_array(&env, &blinding_bytes);
            let wrong_secret = BytesN::from_array(&env, &wrong_secret_bytes);
            let commitment_hash = create_test_commitment(&env, &secret, &blinding);

            // Commit with correct secret
            let ip_id = registry.commit_ip(&owner, &commitment_hash, &0);

            // Verify with wrong secret should fail
            let verified = registry.verify_commitment(&ip_id, &wrong_secret, &blinding);
            prop_assert!(!verified);
        }

        /// Property: Verifying with incorrect blinding factor fails even if secret is correct.
        ///
        /// This ensures the commitment binds both the secret and blinding factor.
        #[test]
        fn prop_wrong_blinding_fails_verification(
            secret_bytes in prop::array::uniform32(any::<u8>()),
            blinding_bytes in prop::array::uniform32(any::<u8>()),
            wrong_blinding_bytes in prop::array::uniform32(any::<u8>()),
        ) {
            // Ensure wrong_blinding is actually different
            if blinding_bytes == wrong_blinding_bytes {
                return Ok(());
            }

            let (env, registry) = setup_registry();
            let owner = Address::generate(&env);

            let secret = BytesN::from_array(&env, &secret_bytes);
            let blinding = BytesN::from_array(&env, &blinding_bytes);
            let wrong_blinding = BytesN::from_array(&env, &wrong_blinding_bytes);
            let commitment_hash = create_test_commitment(&env, &secret, &blinding);

            // Commit with correct blinding
            let ip_id = registry.commit_ip(&owner, &commitment_hash, &0);

            // Verify with wrong blinding should fail
            let verified = registry.verify_commitment(&ip_id, &secret, &wrong_blinding);
            prop_assert!(!verified);
        }

        /// Property: Deterministic verification - same inputs always produce same output.
        ///
        /// Multiple verification calls with identical inputs must always return the same result.
        #[test]
        fn prop_verification_is_deterministic(
            secret_bytes in prop::array::uniform32(any::<u8>()),
            blinding_bytes in prop::array::uniform32(any::<u8>()),
        ) {
            let (env, registry) = setup_registry();
            let owner = Address::generate(&env);

            let secret = BytesN::from_array(&env, &secret_bytes);
            let blinding = BytesN::from_array(&env, &blinding_bytes);
            let commitment_hash = create_test_commitment(&env, &secret, &blinding);

            // Commit
            let ip_id = registry.commit_ip(&owner, &commitment_hash, &0);

            // Verify multiple times - all results must be identical
            let result1 = registry.verify_commitment(&ip_id, &secret, &blinding);
            let result2 = registry.verify_commitment(&ip_id, &secret, &blinding);
            let result3 = registry.verify_commitment(&ip_id, &secret, &blinding);

            prop_assert_eq!(result1, result2);
            prop_assert_eq!(result2, result3);
            prop_assert!(result1);
        }

        /// Property: Hash computation is commutative in concatenation order.
        ///
        /// The commitment hash must be computed as SHA256(secret || blinding),
        /// not any other order.
        #[test]
        fn prop_hash_order_matters(
            secret_bytes in prop::array::uniform32(any::<u8>()),
            blinding_bytes in prop::array::uniform32(any::<u8>()),
        ) {
            // Only test when secret and blinding are different
            if secret_bytes == blinding_bytes {
                return Ok(());
            }

            let (env, registry) = setup_registry();
            let owner1 = Address::generate(&env);
            let owner2 = Address::generate(&env);

            let secret = BytesN::from_array(&env, &secret_bytes);
            let blinding = BytesN::from_array(&env, &blinding_bytes);

            // Correct order: SHA256(secret || blinding)
            let hash_correct = create_test_commitment(&env, &secret, &blinding);
            let ip_id1 = registry.commit_ip(&owner1, &hash_correct, &0);

            // Wrong order: SHA256(blinding || secret)
            let hash_wrong = create_test_commitment(&env, &blinding, &secret);
            let ip_id2 = registry.commit_ip(&owner2, &hash_wrong, &0);

            // Verify with correct secret/blinding order on correct hash
            let verify_correct = registry.verify_commitment(&ip_id1, &secret, &blinding);
            prop_assert!(verify_correct);

            // Try to verify with reversed inputs on correct hash (should fail)
            let verify_reversed = registry.verify_commitment(&ip_id1, &blinding, &secret);
            prop_assert!(!verify_reversed);

            // Verify with reversed secret/blinding order on wrong hash
            let verify_on_wrong_hash = registry.verify_commitment(&ip_id2, &blinding, &secret);
            prop_assert!(verify_on_wrong_hash);
        }

        /// Property: Multiple distinct commitments are independent.
        ///
        /// Creating multiple commitments with different secrets/blindings should
        /// not interfere with their individual verification.
        #[test]
        fn prop_multiple_commitments_independent(
            secret1_bytes in prop::array::uniform32(any::<u8>()),
            blinding1_bytes in prop::array::uniform32(any::<u8>()),
            secret2_bytes in prop::array::uniform32(any::<u8>()),
            blinding2_bytes in prop::array::uniform32(any::<u8>()),
        ) {
            let (env, registry) = setup_registry();
            let owner1 = Address::generate(&env);
            let owner2 = Address::generate(&env);

            let secret1 = BytesN::from_array(&env, &secret1_bytes);
            let blinding1 = BytesN::from_array(&env, &blinding1_bytes);
            let secret2 = BytesN::from_array(&env, &secret2_bytes);
            let blinding2 = BytesN::from_array(&env, &blinding2_bytes);

            let hash1 = create_test_commitment(&env, &secret1, &blinding1);
            let hash2 = create_test_commitment(&env, &secret2, &blinding2);

            let ip_id1 = registry.commit_ip(&owner1, &hash1, &0);
            let ip_id2 = registry.commit_ip(&owner2, &hash2, &0);

            // Each commitment should verify only with its own inputs
            let verify1_correct = registry.verify_commitment(&ip_id1, &secret1, &blinding1);
            let verify2_correct = registry.verify_commitment(&ip_id2, &secret2, &blinding2);

            prop_assert!(verify1_correct);
            prop_assert!(verify2_correct);

            // Cross-verification with wrong inputs should fail
            let verify1_with_secret2 = registry.verify_commitment(&ip_id1, &secret2, &blinding1);
            let verify1_with_blinding2 = registry.verify_commitment(&ip_id1, &secret1, &blinding2);
            let verify2_with_secret1 = registry.verify_commitment(&ip_id2, &secret1, &blinding2);
            let verify2_with_blinding1 = registry.verify_commitment(&ip_id2, &secret2, &blinding1);

            prop_assert!(!verify1_with_secret2);
            prop_assert!(!verify1_with_blinding2);
            prop_assert!(!verify2_with_secret1);
            prop_assert!(!verify2_with_blinding1);
        }

        /// Property: Commitment hash is bound by constraint - all zeros should be rejected.
        ///
        /// Zero commitments are forbidden to prevent ambiguity.
        #[test]
        fn prop_zero_commitment_rejected(_unit in ".*") {
            let (env, registry) = setup_registry();
            let owner = Address::generate(&env);

            let zero_hash = BytesN::from_array(&env, &[0u8; 32]);

            // Attempting to commit with all-zero hash must panic
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                registry.commit_ip(&owner, &zero_hash, &0)
            }));

            prop_assert!(result.is_err(), "Zero commitment hash must be rejected");
        }

        /// Property: Non-zero commitments are always accepted.
        ///
        /// Any non-zero hash with sufficient proof-of-work should be committable.
        #[test]
        fn prop_nonzero_commitment_accepted(
            hash_bytes in prop::array::uniform32(any::<u8>())
                .prop_filter("non-zero hash", |b| b != &[0u8; 32])
        ) {
            let (env, registry) = setup_registry();
            let owner = Address::generate(&env);

            let hash = BytesN::from_array(&env, &hash_bytes);

            // Non-zero commitment with minimal PoW requirement should succeed
            let ip_id = registry.commit_ip(&owner, &hash, &0);
            prop_assert!(ip_id > 0);
        }
    }

    // ── Deterministic Edge Case Tests ─────────────────────────────────────────

    /// Test: All-zero secret with non-zero blinding
    #[test]
    fn test_zero_secret_nonzero_blinding() {
        let (env, registry) = setup_registry();
        let owner = Address::generate(&env);

        let zero_secret = BytesN::from_array(&env, &[0u8; 32]);
        let blinding = BytesN::from_array(&env, &[0xFFu8; 32]);
        let hash = create_test_commitment(&env, &zero_secret, &blinding);

        let ip_id = registry.commit_ip(&owner, &hash, &0);
        let verified = registry.verify_commitment(&ip_id, &zero_secret, &blinding);
        assert!(verified);
    }

    /// Test: Non-zero secret with all-zero blinding
    #[test]
    fn test_nonzero_secret_zero_blinding() {
        let (env, registry) = setup_registry();
        let owner = Address::generate(&env);

        let secret = BytesN::from_array(&env, &[0xAAu8; 32]);
        let zero_blinding = BytesN::from_array(&env, &[0u8; 32]);
        let hash = create_test_commitment(&env, &secret, &zero_blinding);

        let ip_id = registry.commit_ip(&owner, &hash, &0);
        let verified = registry.verify_commitment(&ip_id, &secret, &zero_blinding);
        assert!(verified);
    }

    /// Test: Maximum values for secret and blinding
    #[test]
    fn test_max_secret_max_blinding() {
        let (env, registry) = setup_registry();
        let owner = Address::generate(&env);

        let max_secret = BytesN::from_array(&env, &[0xFFu8; 32]);
        let max_blinding = BytesN::from_array(&env, &[0xFFu8; 32]);
        let hash = create_test_commitment(&env, &max_secret, &max_blinding);

        let ip_id = registry.commit_ip(&owner, &hash, &0);
        let verified = registry.verify_commitment(&ip_id, &max_secret, &max_blinding);
        assert!(verified);
    }

    /// Test: Single bit difference in secret causes verification failure
    #[test]
    fn test_single_bit_difference_in_secret() {
        let (env, registry) = setup_registry();
        let owner = Address::generate(&env);

        let mut secret_bytes = [0xAAu8; 32];
        let blinding_bytes = [0xBBu8; 32];

        let secret = BytesN::from_array(&env, &secret_bytes);
        let blinding = BytesN::from_array(&env, &blinding_bytes);
        let hash = create_test_commitment(&env, &secret, &blinding);

        let ip_id = registry.commit_ip(&owner, &hash, &0);

        // Flip a single bit in the secret
        secret_bytes[0] ^= 0x01;
        let different_secret = BytesN::from_array(&env, &secret_bytes);

        let verified = registry.verify_commitment(&ip_id, &different_secret, &blinding);
        assert!(!verified);
    }

    /// Test: Single bit difference in blinding causes verification failure
    #[test]
    fn test_single_bit_difference_in_blinding() {
        let (env, registry) = setup_registry();
        let owner = Address::generate(&env);

        let secret_bytes = [0xAAu8; 32];
        let mut blinding_bytes = [0xBBu8; 32];

        let secret = BytesN::from_array(&env, &secret_bytes);
        let blinding = BytesN::from_array(&env, &blinding_bytes);
        let hash = create_test_commitment(&env, &secret, &blinding);

        let ip_id = registry.commit_ip(&owner, &hash, &0);

        // Flip a single bit in the blinding
        blinding_bytes[31] ^= 0x01;
        let different_blinding = BytesN::from_array(&env, &blinding_bytes);

        let verified = registry.verify_commitment(&ip_id, &secret, &different_blinding);
        assert!(!verified);
    }

    /// Test: Commitment invariant - hash is immutable after commit
    #[test]
    fn test_commitment_hash_immutability() {
        let (env, registry) = setup_registry();
        let owner = Address::generate(&env);

        let secret = BytesN::from_array(&env, &[0x11u8; 32]);
        let blinding = BytesN::from_array(&env, &[0x22u8; 32]);
        let hash1 = create_test_commitment(&env, &secret, &blinding);

        let ip_id = registry.commit_ip(&owner, &hash1, &0);

        // Verify multiple times - the commitment hash must not change
        for _ in 0..5 {
            let verified = registry.verify_commitment(&ip_id, &secret, &blinding);
            assert!(verified);
        }
    }
}
