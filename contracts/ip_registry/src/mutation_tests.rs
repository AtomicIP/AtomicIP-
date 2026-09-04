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

    // ── Co-ownership (#348) ────────────────────────────────────────────────────

    /// Mutation: skip storing the co-owner → get_ownership_shares still returns
    /// only the primary owner with 100%.
    #[test]
    fn add_co_owner_transfers_percentage_from_primary_owner() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let co = Address::generate(&e);

        let id = c.commit_ip(&owner, &hash(&e, 0x50), &0u32);
        c.add_co_owner(&id, &co, &30u32);

        let shares = c.get_ownership_shares(&id);
        // Primary owner must have dropped from 100 to 70
        let owner_share = shares.iter().find(|s| s.address == owner).unwrap();
        assert_eq!(owner_share.percentage, 70, "Primary owner should hold 70% after adding co-owner with 30%");
        // Co-owner must hold 30%
        let co_share = shares.iter().find(|s| s.address == co).unwrap();
        assert_eq!(co_share.percentage, 30, "Co-owner should hold exactly the assigned 30%");
    }

    /// Mutation: allow percentage = 0 → should panic.
    #[test]
    #[should_panic]
    fn add_co_owner_rejects_zero_percentage() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let co = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x51), &0u32);
        c.add_co_owner(&id, &co, &0u32); // must panic
    }

    /// Mutation: allow percentage > owner's current share → should panic.
    #[test]
    #[should_panic]
    fn add_co_owner_rejects_percentage_exceeding_owner_share() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let co = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x52), &0u32);
        c.add_co_owner(&id, &co, &101u32); // > 100 — must panic
    }

    /// Mutation: skip removing co-owner from co_owners list → list still contains
    /// the removed entry.
    #[test]
    fn remove_co_owner_removes_entry_from_list() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let co = Address::generate(&e);

        let id = c.commit_ip(&owner, &hash(&e, 0x53), &0u32);
        c.add_co_owner(&id, &co, &25u32);
        c.remove_co_owner(&id, &co);

        let shares = c.get_ownership_shares(&id);
        let co_share = shares.iter().find(|s| s.address == co);
        assert!(co_share.is_none(), "Co-owner must be absent after removal");

        // Primary owner must have been restored to 100%
        let owner_share = shares.iter().find(|s| s.address == owner).unwrap();
        assert_eq!(owner_share.percentage, 100, "Primary owner must be restored to 100% after co-owner removal");
    }

    /// Mutation: removing a non-existent co-owner should panic.
    #[test]
    #[should_panic]
    fn remove_co_owner_rejects_unknown_co_owner() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let stranger = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x54), &0u32);
        c.remove_co_owner(&id, &stranger); // must panic
    }

    // ── Versioning (#version) ──────────────────────────────────────────────────

    /// Mutation: forget to set parent_ip_id on the new version record →
    /// get_ip_lineage returns empty.
    #[test]
    fn create_ip_version_sets_parent_link() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);

        let parent_id = c.commit_ip(&owner, &hash(&e, 0x60), &0u32);
        let version_id = c.create_ip_version(&parent_id, &hash(&e, 0x61));

        // The version must exist and reference the parent
        let version_record = c.get_ip(&version_id);
        assert_eq!(
            version_record.parent_ip_id,
            Some(parent_id),
            "New version must link back to parent"
        );
    }

    /// Mutation: allow zero-hash in create_ip_version → should panic.
    #[test]
    #[should_panic]
    fn create_ip_version_rejects_zero_hash() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let parent_id = c.commit_ip(&owner, &hash(&e, 0x62), &0u32);
        c.create_ip_version(&parent_id, &hash(&e, 0x00)); // zero hash must panic
    }

    /// Mutation: allow duplicate hash in create_ip_version → should panic.
    #[test]
    #[should_panic]
    fn create_ip_version_rejects_duplicate_hash() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let h = hash(&e, 0x63);
        let parent_id = c.commit_ip(&owner, &h, &0u32);
        // Attempt to create a version with the same hash as the parent
        c.create_ip_version(&parent_id, &h); // duplicate must panic
    }

    /// Mutation: increment ID counter by 0 → both parent and version get same ID.
    #[test]
    fn create_ip_version_uses_new_unique_id() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);

        let parent_id = c.commit_ip(&owner, &hash(&e, 0x64), &0u32);
        let version_id = c.create_ip_version(&parent_id, &hash(&e, 0x65));

        assert_ne!(version_id, parent_id, "Version ID must differ from parent ID");
        assert!(version_id > parent_id, "Version ID must be greater than parent ID (sequential)");
    }

    /// Mutation: skip appending version_id to get_ip_versions list.
    #[test]
    fn get_ip_versions_contains_new_version() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);

        let parent_id = c.commit_ip(&owner, &hash(&e, 0x66), &0u32);
        let v1 = c.create_ip_version(&parent_id, &hash(&e, 0x67));
        let v2 = c.create_ip_version(&parent_id, &hash(&e, 0x68));

        let versions = c.get_ip_versions(&parent_id);
        assert!(versions.iter().any(|id| id == v1), "v1 must be in get_ip_versions");
        assert!(versions.iter().any(|id| id == v2), "v2 must be in get_ip_versions");
    }

    // ── Notarization (#345) ────────────────────────────────────────────────────

    // NOTE: notarize_ip_timestamp performs an Ed25519 signature check that
    // requires a real cryptographic key, which is impractical in a unit test
    // without mocking the crypto host function.  The tests below instead confirm
    // the *guard* paths (key not configured, wrong signature length) that
    // mutations could remove, and verify that get_ip_notary_signature correctly
    // returns None before notarization.

    /// Mutation: skip the "notary key not configured" guard → call succeeds
    /// without a key, bypassing verification.
    #[test]
    #[should_panic]
    fn notarize_ip_timestamp_panics_without_configured_key() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x70), &0u32);

        // No set_notary_public_key call → must panic (key not configured)
        let sig = soroban_sdk::Bytes::from_array(&e, &[0xABu8; 64]);
        c.notarize_ip_timestamp(&id, &sig);
    }

    /// Mutation: skip the "signature != 64 bytes" guard → short sig accepted.
    #[test]
    #[should_panic]
    fn notarize_ip_timestamp_panics_with_wrong_length_signature() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x71), &0u32);

        // Set a dummy notary key so the key-presence guard passes
        let notary_key = BytesN::from_array(&e, &[0x01u8; 32]);
        c.set_notary_public_key(&notary_key);

        // 32 bytes instead of 64 → must panic (wrong length)
        let short_sig = soroban_sdk::Bytes::from_array(&e, &[0xABu8; 32]);
        c.notarize_ip_timestamp(&id, &short_sig);
    }

    /// Mutation: always return Some(…) from get_ip_notary_signature even before
    /// notarization has occurred.
    #[test]
    fn get_ip_notary_signature_returns_none_before_notarization() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x72), &0u32);

        assert!(
            c.get_ip_notary_signature(&id).is_none(),
            "Notary signature must be None before notarization"
        );
    }

    // ── Ownership Challenges (#811) ────────────────────────────────────────────

    /// Mutation: skip storing the challenge → get_ownership_challenge returns None.
    #[test]
    fn issue_ownership_challenge_stores_challenge() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let challenger = Address::generate(&e);

        let id = c.commit_ip(&owner, &hash(&e, 0x80), &0u32);
        let nonce = BytesN::from_array(&e, &[0x01u8; 32]);
        let challenge_id = c.issue_ownership_challenge(&id, &challenger, &nonce);

        let stored = c.get_ownership_challenge(&challenge_id);
        assert!(stored.is_some(), "Challenge must be stored after issue_ownership_challenge");

        let ch = stored.unwrap();
        assert_eq!(ch.ip_id, id, "Challenge ip_id must match");
        assert_eq!(ch.challenger, challenger, "Challenge challenger must match");
        assert!(!ch.verified, "New challenge must not be marked verified");
        assert!(ch.response_hash.is_none(), "New challenge must have no response yet");
    }

    /// Mutation: start challenge IDs at 0 instead of 1.
    #[test]
    fn issue_ownership_challenge_ids_are_positive() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let challenger = Address::generate(&e);

        let id = c.commit_ip(&owner, &hash(&e, 0x81), &0u32);
        let nonce = BytesN::from_array(&e, &[0x02u8; 32]);
        let challenge_id = c.issue_ownership_challenge(&id, &challenger, &nonce);

        assert!(challenge_id >= 1, "Challenge ID must be >= 1 (first valid ID)");
    }

    /// Mutation: skip writing response_hash to storage in
    /// respond_to_ownership_challenge.
    #[test]
    fn respond_to_ownership_challenge_stores_response() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let challenger = Address::generate(&e);

        let ip_id = c.commit_ip(&owner, &hash(&e, 0x82), &0u32);
        let nonce = BytesN::from_array(&e, &[0x03u8; 32]);
        let challenge_id = c.issue_ownership_challenge(&ip_id, &challenger, &nonce);

        // IP owner responds with an arbitrary response hash
        let response = BytesN::from_array(&e, &[0xBBu8; 32]);
        c.respond_to_ownership_challenge(&challenge_id, &response);

        let ch = c.get_ownership_challenge(&challenge_id).unwrap();
        assert!(
            ch.response_hash.is_some(),
            "Response hash must be stored after respond_to_ownership_challenge"
        );
        assert_eq!(
            ch.response_hash.unwrap(),
            response,
            "Stored response hash must match the submitted response"
        );
    }

    /// Mutation: flip the boolean logic in verify_ownership_challenge so it
    /// always returns true (even with a wrong response).
    #[test]
    fn verify_ownership_challenge_rejects_incorrect_response() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let challenger = Address::generate(&e);

        let ip_id = c.commit_ip(&owner, &hash(&e, 0x83), &0u32);
        let nonce = BytesN::from_array(&e, &[0x04u8; 32]);
        let challenge_id = c.issue_ownership_challenge(&ip_id, &challenger, &nonce);

        // Submit a clearly wrong response
        let wrong_response = BytesN::from_array(&e, &[0xFFu8; 32]);
        c.respond_to_ownership_challenge(&challenge_id, &wrong_response);

        let valid = c.verify_ownership_challenge(&challenge_id);
        assert!(!valid, "verify_ownership_challenge must return false for an incorrect response");
    }

    /// Mutation: allow expire_challenge to fire before the TTL elapses.
    #[test]
    #[should_panic]
    fn expire_challenge_panics_before_ttl_elapsed() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let challenger = Address::generate(&e);

        let ip_id = c.commit_ip(&owner, &hash(&e, 0x84), &0u32);
        let nonce = BytesN::from_array(&e, &[0x05u8; 32]);
        let challenge_id = c.issue_ownership_challenge(&ip_id, &challenger, &nonce);

        // Attempt to expire immediately — TTL has not elapsed → must panic
        c.expire_challenge(&challenge_id);
    }

    /// Mutation: skip removing the challenge from storage in expire_challenge →
    /// challenge remains accessible after expiry.
    #[test]
    fn expire_challenge_removes_challenge_from_storage() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let challenger = Address::generate(&e);

        let ip_id = c.commit_ip(&owner, &hash(&e, 0x85), &0u32);
        let nonce = BytesN::from_array(&e, &[0x06u8; 32]);
        let challenge_id = c.issue_ownership_challenge(&ip_id, &challenger, &nonce);

        // Advance ledger time past the default TTL (86 400 s)
        e.ledger().with_mut(|l| {
            l.timestamp += 86_401;
        });

        c.expire_challenge(&challenge_id);
        assert!(
            c.get_ownership_challenge(&challenge_id).is_none(),
            "Challenge must be absent from storage after expire_challenge"
        );
    }
}
