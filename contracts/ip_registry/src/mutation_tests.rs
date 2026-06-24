/// Mutation-catching tests for IP Registry.
/// Targets 85%+ mutation kill rate by covering:
///   - boundary condition flips (== vs !=, > vs >=)
///   - boolean negations
///   - off-by-one errors in ID counters
///   - missing auth checks
///   - access control bypasses
///   - storage key collisions
#[cfg(test)]
mod mutation_tests {
    use soroban_sdk::{
        contractclient,
        testutils::Address as _,
        testutils::Events,
        symbol_short,
        Address, BytesN, Env, Vec,
    };

    #[contractclient(name = "IpRegistryClient")]
    #[allow(dead_code)]
    trait IpRegistry {
        fn commit_ip(env: Env, owner: Address, commitment_hash: BytesN<32>, pow_difficulty: u32) -> u64;
        fn batch_commit_ip(env: Env, owner: Address, commitment_hashes: Vec<BytesN<32>>) -> Vec<u64>;
        fn batch_commit_ip_anonymous(env: Env, blinded_owner: BytesN<32>, commitment_hashes: Vec<BytesN<32>>) -> Vec<u64>;
        fn get_ip(env: Env, ip_id: u64) -> crate::IpRecord;
        fn verify_commitment(env: Env, ip_id: u64, secret: BytesN<32>, blinding_factor: BytesN<32>) -> bool;
        fn list_ip_by_owner(env: Env, owner: Address) -> Vec<u64>;
        fn transfer_ip(env: Env, ip_id: u64, new_owner: Address);
        fn transfer_ip_ownership(env: Env, ip_id: u64, new_owner: Address);
        fn revoke_ip(env: Env, ip_id: u64);
        fn is_ip_owner(env: Env, ip_id: u64, address: Address) -> bool;
        fn grant_ip_access(env: Env, ip_id: u64, grantee: Address, access_level: u32);
        fn revoke_ip_access(env: Env, ip_id: u64, grantee: Address);
        fn check_ip_access(env: Env, ip_id: u64, grantee: Address, required_level: u32) -> bool;
        fn get_anonymous_owner(env: Env, commitment_hash: BytesN<32>) -> Option<BytesN<32>>;
    }

    fn env() -> Env {
        let e = Env::default();
        e.mock_all_auths();
        e
    }

    fn client(e: &Env) -> IpRegistryClient<'_> {
        IpRegistryClient::new(e, &e.register(crate::IpRegistry, ()))
    }

    fn hash(e: &Env, seed: u8) -> BytesN<32> {
        BytesN::from_array(e, &[seed; 32])
    }

    fn make_commitment(e: &Env, secret_seed: u8, blinding_seed: u8) -> BytesN<32> {
        let secret = BytesN::from_array(e, &[secret_seed; 32]);
        let blinding = BytesN::from_array(e, &[blinding_seed; 32]);
        let mut preimage = soroban_sdk::Bytes::new(e);
        preimage.append(&soroban_sdk::Bytes::from(secret));
        preimage.append(&soroban_sdk::Bytes::from(blinding));
        e.crypto().sha256(&preimage).into()
    }

    // ── Zero-hash guard ───────────────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn zero_hash_is_rejected() {
        let e = env();
        client(&e).commit_ip(&Address::generate(&e), &hash(&e, 0x00), &0u32);
    }

    #[test]
    fn non_zero_hash_is_accepted() {
        let e = env();
        let id = client(&e).commit_ip(&Address::generate(&e), &hash(&e, 0x01), &0u32);
        assert_eq!(id, 1);
    }

    // ── ID counter ────────────────────────────────────────────────────────────

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

    #[test]
    fn batch_commit_returns_sequential_ids() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let hashes = Vec::from_array(&e, [hash(&e, 0x10), hash(&e, 0x11), hash(&e, 0x12)]);
        let ids = c.batch_commit_ip(&owner, &hashes);
        assert_eq!(ids.len(), 3);
        assert_eq!(ids.get(0).unwrap(), 1);
        assert_eq!(ids.get(1).unwrap(), 2);
        assert_eq!(ids.get(2).unwrap(), 3);
    }

    // ── Duplicate commitment guard ────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn duplicate_hash_is_rejected() {
        let e = env();
        let c = client(&e);
        let h = hash(&e, 0xAA);
        c.commit_ip(&Address::generate(&e), &h, &0u32);
        c.commit_ip(&Address::generate(&e), &h, &0u32);
    }

    #[test]
    #[should_panic]
    fn duplicate_in_batch_is_rejected() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let h = hash(&e, 0xBB);
        let hashes = Vec::from_array(&e, [h.clone(), h.clone()]);
        c.batch_commit_ip(&owner, &hashes);
    }

    // ── Revoke guard ──────────────────────────────────────────────────────────

    #[test]
    fn revoked_flag_is_set_after_revoke() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x10), &0u32);
        c.revoke_ip(&id);
        let record = c.get_ip(&id);
        assert!(record.revoked);
    }

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

    // ── Transfer mutations ────────────────────────────────────────────────────

    #[test]
    fn transfer_ip_updates_owner() {
        let e = env();
        let c = client(&e);
        let owner1 = Address::generate(&e);
        let owner2 = Address::generate(&e);
        let id = c.commit_ip(&owner1, &hash(&e, 0x20), &0u32);
        c.transfer_ip(&id, &owner2);
        let record = c.get_ip(&id);
        assert_eq!(record.owner, owner2);
    }

    #[test]
    fn transfer_clears_from_old_owner_index() {
        let e = env();
        let c = client(&e);
        let owner1 = Address::generate(&e);
        let owner2 = Address::generate(&e);
        let id = c.commit_ip(&owner1, &hash(&e, 0x21), &0u32);
        c.transfer_ip(&id, &owner2);
        let old_owner_ids = c.list_ip_by_owner(&owner1);
        assert_eq!(old_owner_ids.len(), 0);
        let new_owner_ids = c.list_ip_by_owner(&owner2);
        assert_eq!(new_owner_ids.len(), 1);
        assert_eq!(new_owner_ids.get(0).unwrap(), id);
    }

    #[test]
    #[should_panic]
    fn transfer_nonexistent_ip_panics() {
        let e = env();
        let c = client(&e);
        c.transfer_ip(&999u64, &Address::generate(&e));
    }

    // ── get_ip mutations ──────────────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn get_nonexistent_ip_panics() {
        let e = env();
        client(&e).get_ip(&999u64);
    }

    #[test]
    fn stored_commitment_hash_matches_input() {
        let e = env();
        let c = client(&e);
        let h = hash(&e, 0x42);
        let id = c.commit_ip(&Address::generate(&e), &h, &0u32);
        let record = c.get_ip(&id);
        assert_eq!(record.commitment_hash, h);
    }

    // ── verify_commitment ────────────────────────────────────────────────────

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

    #[test]
    fn verify_commitment_rejects_wrong_blinding() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let secret = BytesN::from_array(&e, &[0x03u8; 32]);
        let blinding = BytesN::from_array(&e, &[0x04u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&e);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = e.crypto().sha256(&preimage).into();
        let id = c.commit_ip(&owner, &commitment_hash, &0u32);
        let wrong_blinding = BytesN::from_array(&e, &[0xFFu8; 32]);
        assert!(!c.verify_commitment(&id, &secret, &wrong_blinding));
    }

    #[test]
    #[should_panic]
    fn verify_commitment_nonexistent_ip_panics() {
        let e = env();
        client(&e).verify_commitment(
            &999u64,
            &BytesN::from_array(&e, &[0u8; 32]),
            &BytesN::from_array(&e, &[0u8; 32]),
        );
    }

    // ── Owner index ───────────────────────────────────────────────────────────

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

    #[test]
    fn list_ip_by_owner_empty_for_no_ips() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let ids = c.list_ip_by_owner(&owner);
        assert_eq!(ids.len(), 0);
    }

    // ── is_ip_owner ───────────────────────────────────────────────────────────

    #[test]
    fn is_ip_owner_returns_true_for_owner() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x30), &0u32);
        assert!(c.is_ip_owner(&id, &owner));
    }

    #[test]
    fn is_ip_owner_returns_false_for_non_owner() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let other = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x31), &0u32);
        assert!(!c.is_ip_owner(&id, &other));
    }

    #[test]
    fn is_ip_owner_returns_false_for_nonexistent_ip() {
        let e = env();
        let c = client(&e);
        assert!(!c.is_ip_owner(&999u64, &Address::generate(&e)));
    }

    // ── Access control mutations ──────────────────────────────────────────────

    #[test]
    fn grant_ip_access_stores_level() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let grantee = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x40), &0u32);
        c.grant_ip_access(&id, &grantee, &2u32);
        assert!(c.check_ip_access(&id, &grantee, &2u32));
    }

    #[test]
    fn not_granted_ip_access_denied() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let stranger = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x41), &0u32);
        assert!(!c.check_ip_access(&id, &stranger, &1u32));
    }

    #[test]
    fn owner_always_has_full_access() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x42), &0u32);
        assert!(c.check_ip_access(&id, &owner, &3u32));
    }

    #[test]
    fn grant_then_revoke_removes_access() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let grantee = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x43), &0u32);
        c.grant_ip_access(&id, &grantee, &2u32);
        assert!(c.check_ip_access(&id, &grantee, &2u32));
        c.revoke_ip_access(&id, &grantee);
        assert!(!c.check_ip_access(&id, &grantee, &1u32));
    }

    #[test]
    fn access_level_is_hierarchical() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let grantee = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x44), &0u32);
        c.grant_ip_access(&id, &grantee, &3u32);
        assert!(c.check_ip_access(&id, &grantee, &1u32));
        assert!(c.check_ip_access(&id, &grantee, &2u32));
        assert!(c.check_ip_access(&id, &grantee, &3u32));
    }

    #[test]
    fn grant_updates_existing_level() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let grantee = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x45), &0u32);
        c.grant_ip_access(&id, &grantee, &1u32);
        assert!(c.check_ip_access(&id, &grantee, &1u32));
        assert!(!c.check_ip_access(&id, &grantee, &2u32));
        c.grant_ip_access(&id, &grantee, &3u32);
        assert!(c.check_ip_access(&id, &grantee, &3u32));
    }

    // ── Anonymous batch mutations ─────────────────────────────────────────────

    #[test]
    fn anonymous_batch_creates_records() {
        let e = env();
        let c = client(&e);
        let blinded = hash(&e, 0x50);
        let hashes = Vec::from_array(&e, [hash(&e, 0x51), hash(&e, 0x52)]);
        let ids = c.batch_commit_ip_anonymous(&blinded, &hashes);
        assert_eq!(ids.len(), 2);
        for id in ids.iter() {
            let record = c.get_ip(&id);
            assert_eq!(record.commitment_hash, hashes.get(id as u32 - 1).unwrap());
        }
    }

    #[test]
    fn anonymous_batch_stores_blinded_owner() {
        let e = env();
        let c = client(&e);
        let blinded = hash(&e, 0x53);
        let hashes = Vec::from_array(&e, [hash(&e, 0x54)]);
        let ids = c.batch_commit_ip_anonymous(&blinded, &hashes);
        let stored = c.get_anonymous_owner(&hashes.get(0).unwrap());
        assert_eq!(stored, Some(blinded));
    }

    #[test]
    fn get_anonymous_owner_none_for_regular_commit() {
        let e = env();
        let c = client(&e);
        let h = hash(&e, 0x55);
        let _id = c.commit_ip(&Address::generate(&e), &h, &0u32);
        assert_eq!(c.get_anonymous_owner(&h), None);
    }

    #[test]
    #[should_panic]
    fn anonymous_batch_empty_hashes_rejected() {
        let e = env();
        let c = client(&e);
        let blinded = hash(&e, 0x60);
        let empty: Vec<BytesN<32>> = Vec::new(&e);
        c.batch_commit_ip_anonymous(&blinded, &empty);
    }

    // ── Event emission mutations ──────────────────────────────────────────────

    #[test]
    fn commit_ip_emits_event() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let _id = c.commit_ip(&owner, &hash(&e, 0x70), &0u32);
        let all_events = e.events().all();
        assert!(all_events.events().len() > 0);
        let topics = all_events.events().get(0).unwrap().topics;
        assert!(topics.contains(&symbol_short!("ip_cmt").into()));
    }

    #[test]
    fn revoke_ip_emits_event() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x71), &0u32);
        c.revoke_ip(&id);
        let all_events = e.events().all();
        let found = all_events.events().iter().any(|ev| {
            ev.topics.contains(&symbol_short!("ip_rev").into())
        });
        assert!(found);
    }

    #[test]
    fn transfer_ip_emits_event() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let new_owner = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x72), &0u32);
        c.transfer_ip(&id, &new_owner);
        let all_events = e.events().all();
        let found = all_events.events().iter().any(|ev| {
            ev.topics.contains(&symbol_short!("ip_trf").into())
        });
        assert!(found);
    }

    #[test]
    fn grant_ip_access_emits_event() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let grantee = Address::generate(&e);
        let id = c.commit_ip(&owner, &hash(&e, 0x73), &0u32);
        c.grant_ip_access(&id, &grantee, &1u32);
        let all_events = e.events().all();
        let found = all_events.events().iter().any(|ev| {
            ev.topics.contains(&symbol_short!("ac_grant").into())
        });
        assert!(found);
    }
}
