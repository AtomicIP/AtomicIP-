/// #378 Mutation-catching tests for IP Registry.
///
/// These tests are specifically designed to kill common mutations:
///   - boundary condition flips (== vs !=, > vs >=)
///   - boolean negations
///   - off-by-one errors in ID counters
///   - missing auth checks
#[cfg(test)]
mod mutation_tests {
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

    use crate::{IpRegistry, IpRegistryClient};

    fn env() -> Env {
        let e = Env::default();
        e.mock_all_auths();
        e
    }

    fn client(e: &Env) -> IpRegistryClient<'_> {
        IpRegistryClient::new(e, &e.register(IpRegistry, ()))
    }

    fn hash(e: &Env, seed: u8) -> BytesN<32> {
        BytesN::from_array(e, &[seed; 32])
    }

    // ── Zero-hash guard ───────────────────────────────────────────────────────

    /// Mutation: remove the zero-hash check → this test catches it.
    #[test]
    #[should_panic]
    fn zero_hash_is_rejected() {
        let e = env();
        client(&e).commit_ip(&Address::generate(&e), &hash(&e, 0x00), &0u32);
    }

    /// Non-zero hash must succeed (guards against over-eager rejection).
    #[test]
    fn non_zero_hash_is_accepted() {
        let e = env();
        let id = client(&e).commit_ip(&Address::generate(&e), &hash(&e, 0x01), &0u32);
        assert_eq!(id, 1, "first IP ID must be 1");
    }

    // ── ID counter ────────────────────────────────────────────────────────────

    /// Mutation: id + 0 instead of id + 1 → counter never advances.
    #[test]
    fn ids_are_sequential_starting_at_one() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let id1 = c.commit_ip(&owner, &hash(&e, 0x01), &0u32);
        let id2 = c.commit_ip(&owner, &hash(&e, 0x02), &0u32);
        let id3 = c.commit_ip(&owner, &hash(&e, 0x03), &0u32);
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
    }

    // ── Duplicate commitment guard ────────────────────────────────────────────

    /// Mutation: remove duplicate check → second commit with same hash succeeds.
    #[test]
    #[should_panic]
    fn duplicate_hash_is_rejected() {
        let e = env();
        let c = client(&e);
        let h = hash(&e, 0xAA);
        c.commit_ip(&Address::generate(&e), &h, &0u32);
        c.commit_ip(&Address::generate(&e), &h, &0u32);
    }

    // ── Revoke guard ──────────────────────────────────────────────────────────

    /// Mutation: flip `revoked = true` to `revoked = false` → record stays active.
    #[test]
    fn revoked_flag_is_set_after_revoke() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x10), &0u32);
        c.revoke_ip(&id);
        let record = c.get_ip(&id);
        assert!(record.revoked, "record must be marked revoked");
    }

    /// Mutation: allow double-revoke → this test catches it.
    #[test]
    #[should_panic]
    fn double_revoke_is_rejected() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x11), &0u32);
        c.revoke_ip(&id);
        c.revoke_ip(&id);
    }

    // ── Owner index ───────────────────────────────────────────────────────────

    /// Mutation: skip appending to OwnerIps → list_ip_by_owner returns empty.
    #[test]
    fn owner_index_contains_committed_ids() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let id1 = c.commit_ip(&owner, &hash(&e, 0x20), &0u32);
        let id2 = c.commit_ip(&owner, &hash(&e, 0x21), &0u32);
        let ids = c.list_ip_by_owner(&owner);
        assert_eq!(ids.len(), 2);
        assert_eq!(ids.get(0).unwrap(), id1);
        assert_eq!(ids.get(1).unwrap(), id2);
    }

    // ── Commitment hash stored correctly ──────────────────────────────────────

    /// Mutation: store wrong hash in IpRecord → get_ip returns wrong hash.
    #[test]
    fn stored_commitment_hash_matches_input() {
        let e = env();
        let c = client(&e);
        let h = hash(&e, 0x42);
        let id = c.commit_ip(&Address::generate(&e), &h, &0u32);
        let record = c.get_ip(&id);
        assert_eq!(record.commitment_hash, h);
    }

    // ── verify_commitment ─────────────────────────────────────────────────────

    /// Mutation: always return true from verify_commitment.
    #[test]
    fn verify_commitment_rejects_wrong_secret() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);

        let secret = BytesN::from_array(&e, &[0x01u8; 32]);
        let blinding = BytesN::from_array(&e, &[0x02u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&e);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = e.crypto().sha256(&preimage).into();
        let id = c.commit_ip(&owner, &commitment_hash, &0u32);

        let wrong_secret = BytesN::from_array(&e, &[0xFFu8; 32]);
        assert!(!c.verify_commitment(&id, &wrong_secret, &blinding));
    }

    /// Mutation: always return false from verify_commitment.
    #[test]
    fn verify_commitment_accepts_correct_secret() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);

        let secret = BytesN::from_array(&e, &[0x01u8; 32]);
        let blinding = BytesN::from_array(&e, &[0x02u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&e);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = e.crypto().sha256(&preimage).into();
        let id = c.commit_ip(&owner, &commitment_hash, &0u32);

        assert!(c.verify_commitment(&id, &secret, &blinding));
    }
}
