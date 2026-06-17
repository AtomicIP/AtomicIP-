/// #549 Contract Invariant Testing — IP Registry
///
/// Property-based tests verifying that the documented invariants (docs/invariants.md)
/// hold for all reachable states of the IP Registry contract.
///
/// Run with: cargo test invariant_ -p ip_registry
#[cfg(test)]
mod invariant_tests {
    use proptest::prelude::*;
    use soroban_sdk::{
        testutils::Address as _,
        Address, Bytes, BytesN, Env,
    };

    use crate::{IpRegistry, IpRegistryClient};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_commitment(env: &Env, secret: &BytesN<32>, blinding: &BytesN<32>) -> BytesN<32> {
        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(secret.clone()));
        preimage.append(&Bytes::from(blinding.clone()));
        env.crypto().sha256(&preimage).into()
    }

    fn setup() -> (Env, IpRegistryClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &id);
        (env, client)
    }

    // ── I1: Commitment Uniqueness ─────────────────────────────────────────────

    proptest! {
        /// I1: The same commitment hash cannot be registered twice by the same owner.
        #[test]
        fn invariant_i1_commitment_uniqueness(seed in 1u8..=254u8) {
            let (env, client) = setup();
            let owner = Address::generate(&env);
            let secret = BytesN::from_array(&env, &[seed; 32]);
            let blinding = BytesN::from_array(&env, &[seed.wrapping_add(1); 32]);
            let hash = make_commitment(&env, &secret, &blinding);

            client.commit_ip(&owner, &hash, &0u32);

            // Second commit with the same hash must be rejected.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                client.commit_ip(&owner, &hash, &0u32);
            }));
            prop_assert!(result.is_err(), "Duplicate commitment must be rejected");
        }

        /// I1: Different owners may not register the same commitment hash.
        #[test]
        fn invariant_i1_commitment_uniqueness_cross_owner(seed in 1u8..=254u8) {
            let (env, client) = setup();
            let owner1 = Address::generate(&env);
            let owner2 = Address::generate(&env);
            let secret = BytesN::from_array(&env, &[seed; 32]);
            let blinding = BytesN::from_array(&env, &[seed.wrapping_add(1); 32]);
            let hash = make_commitment(&env, &secret, &blinding);

            client.commit_ip(&owner1, &hash, &0u32);

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                client.commit_ip(&owner2, &hash, &0u32);
            }));
            prop_assert!(result.is_err(), "Same hash must be rejected for any owner");
        }
    }

    // ── I2: Timestamp Monotonicity ────────────────────────────────────────────

    proptest! {
        /// I2: IP IDs are monotonically increasing, which implies timestamp ordering
        /// because the ledger timestamp is non-decreasing.
        #[test]
        fn invariant_i2_id_monotonicity(n in 2usize..=10usize) {
            let (env, client) = setup();
            let owner = Address::generate(&env);
            let mut ids = Vec::new();

            for i in 0..n {
                let secret = BytesN::from_array(&env, &[i as u8 + 1; 32]);
                let blinding = BytesN::from_array(&env, &[(i as u8).wrapping_add(0x80); 32]);
                let hash = make_commitment(&env, &secret, &blinding);
                ids.push(client.commit_ip(&owner, &hash, &0u32));
            }

            // IDs must be strictly increasing.
            for w in ids.windows(2) {
                prop_assert!(w[1] > w[0], "IP IDs must be monotonically increasing");
            }
        }
    }

    // ── I3: Owner Immutability ────────────────────────────────────────────────

    proptest! {
        /// I3: The owner stored at commit time is always returned by get_ip.
        #[test]
        fn invariant_i3_owner_immutability(seed in 1u8..=254u8) {
            let (env, client) = setup();
            let owner = Address::generate(&env);
            let secret = BytesN::from_array(&env, &[seed; 32]);
            let blinding = BytesN::from_array(&env, &[seed.wrapping_add(1); 32]);
            let hash = make_commitment(&env, &secret, &blinding);

            let ip_id = client.commit_ip(&owner, &hash, &0u32);
            let record = client.get_ip(&ip_id);

            prop_assert_eq!(record.owner, owner, "Owner must match the committing address");
        }
    }

    // ── I4: Commitment Verification ───────────────────────────────────────────

    proptest! {
        /// I4: verify_commitment returns true only for the correct secret+blinding pair.
        #[test]
        fn invariant_i4_correct_secret_verifies(seed in 1u8..=254u8) {
            let (env, client) = setup();
            let owner = Address::generate(&env);
            let secret = BytesN::from_array(&env, &[seed; 32]);
            let blinding = BytesN::from_array(&env, &[seed.wrapping_add(1); 32]);
            let hash = make_commitment(&env, &secret, &blinding);

            let ip_id = client.commit_ip(&owner, &hash, &0u32);
            prop_assert!(
                client.verify_commitment(&ip_id, &secret, &blinding),
                "Correct secret+blinding must verify"
            );
        }

        /// I4: verify_commitment returns false for a wrong secret.
        #[test]
        fn invariant_i4_wrong_secret_fails(seed in 1u8..=253u8) {
            let (env, client) = setup();
            let owner = Address::generate(&env);
            let secret = BytesN::from_array(&env, &[seed; 32]);
            let blinding = BytesN::from_array(&env, &[seed.wrapping_add(1); 32]);
            let hash = make_commitment(&env, &secret, &blinding);

            let ip_id = client.commit_ip(&owner, &hash, &0u32);

            let wrong_secret = BytesN::from_array(&env, &[seed.wrapping_add(2); 32]);
            prop_assert!(
                !client.verify_commitment(&ip_id, &wrong_secret, &blinding),
                "Wrong secret must not verify"
            );
        }
    }

    // ── I5: Zero hash rejected ────────────────────────────────────────────────

    #[test]
    fn invariant_i5_zero_hash_rejected() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let zero_hash = BytesN::from_array(&env, &[0u8; 32]);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.commit_ip(&owner, &zero_hash, &0u32);
        }));
        assert!(result.is_err(), "Zero commitment hash must be rejected");
    }

    // ── I6: list_ip_by_owner consistency ─────────────────────────────────────

    proptest! {
        /// I6: Every committed IP ID appears in list_ip_by_owner for that owner.
        #[test]
        fn invariant_i6_owner_list_consistency(n in 1usize..=5usize) {
            let (env, client) = setup();
            let owner = Address::generate(&env);
            let mut committed_ids = Vec::new();

            for i in 0..n {
                let secret = BytesN::from_array(&env, &[i as u8 + 10; 32]);
                let blinding = BytesN::from_array(&env, &[(i as u8).wrapping_add(0x40); 32]);
                let hash = make_commitment(&env, &secret, &blinding);
                committed_ids.push(client.commit_ip(&owner, &hash, &0u32));
            }

            let listed = client.list_ip_by_owner(&owner);
            for id in &committed_ids {
                prop_assert!(
                    listed.contains(id),
                    "IP {} must appear in owner's list",
                    id
                );
            }
            prop_assert_eq!(
                listed.len() as usize, n,
                "Owner list length must equal number of commits"
            );
        }
    }

    // ── I7: Revoked IP cannot be re-committed ─────────────────────────────────

    #[test]
    fn invariant_i7_revoked_ip_record_preserved() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let secret = BytesN::from_array(&env, &[0xABu8; 32]);
        let blinding = BytesN::from_array(&env, &[0xCDu8; 32]);
        let hash = make_commitment(&env, &secret, &blinding);

        let ip_id = client.commit_ip(&owner, &hash, &0u32);
        client.revoke_ip(&ip_id);

        let record = client.get_ip(&ip_id);
        assert!(record.revoked, "Revoked IP must have revoked=true");
        // The record must still be retrievable (not deleted).
        assert_eq!(record.ip_id, ip_id);
    }
}
