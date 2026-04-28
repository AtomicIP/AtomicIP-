/// #379 Contract State Snapshot Testing — IP Registry
///
/// Verifies contract state after key operations via field-level snapshots.
/// Catches state corruption and unintended side-effects.
#[cfg(test)]
mod snapshot_tests {
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

    use crate::{IpRegistry, IpRegistryClient};

    fn env() -> Env {
        let e = Env::default();
        e.mock_all_auths();
        e
    }

    fn client(e: &Env) -> IpRegistryClient {
        IpRegistryClient::new(e, &e.register(IpRegistry, ()))
    }

    // ── commit_ip snapshot ────────────────────────────────────────────────────

    #[test]
    fn snapshot_after_commit_ip() {
        let e = env();
        let c = client(&e);
        let hash = BytesN::from_array(&e, &[0x42u8; 32]);
        let id = c.commit_ip(&Address::generate(&e), &hash, &0u32);

        let record = c.get_ip(&id);
        assert_eq!(record.ip_id, 1);
        assert!(!record.revoked);
        assert_eq!(record.commitment_hash, hash);
    }

    #[test]
    fn snapshot_two_commits_independent_state() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);
        let h1 = BytesN::from_array(&e, &[0x01u8; 32]);
        let h2 = BytesN::from_array(&e, &[0x02u8; 32]);

        let id1 = c.commit_ip(&owner, &h1, &0u32);
        let id2 = c.commit_ip(&owner, &h2, &0u32);

        let r1 = c.get_ip(&id1);
        let r2 = c.get_ip(&id2);

        assert_eq!(r1.ip_id, 1);
        assert_eq!(r1.commitment_hash, h1);
        assert!(!r1.revoked);

        assert_eq!(r2.ip_id, 2);
        assert_eq!(r2.commitment_hash, h2);
        assert!(!r2.revoked);
    }

    // ── revoke_ip snapshot ────────────────────────────────────────────────────

    #[test]
    fn snapshot_after_revoke_ip() {
        let e = env();
        let c = client(&e);
        let hash = BytesN::from_array(&e, &[0xABu8; 32]);
        let id = c.commit_ip(&Address::generate(&e), &hash, &0u32);

        c.revoke_ip(&id);

        let record = c.get_ip(&id);
        assert!(record.revoked, "snapshot must show revoked=true after revoke_ip");
        assert_eq!(record.commitment_hash, hash, "hash must not change on revoke");
    }

    // ── owner index snapshot ──────────────────────────────────────────────────

    #[test]
    fn snapshot_owner_index_after_three_commits() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);

        let id1 = c.commit_ip(&owner, &BytesN::from_array(&e, &[0x10u8; 32]), &0u32);
        let id2 = c.commit_ip(&owner, &BytesN::from_array(&e, &[0x11u8; 32]), &0u32);
        let id3 = c.commit_ip(&owner, &BytesN::from_array(&e, &[0x12u8; 32]), &0u32);

        let ids = c.list_ip_by_owner(&owner);
        assert_eq!(ids.len(), 3);
        assert_eq!(ids.get(0).unwrap(), id1);
        assert_eq!(ids.get(1).unwrap(), id2);
        assert_eq!(ids.get(2).unwrap(), id3);
    }

    // ── state diff: existing record unchanged after new commit ────────────────

    #[test]
    fn snapshot_existing_record_unchanged_after_new_commit() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);

        let h1 = BytesN::from_array(&e, &[0xAAu8; 32]);
        let id1 = c.commit_ip(&owner, &h1, &0u32);

        // Commit a second IP — must not alter the first record.
        c.commit_ip(&owner, &BytesN::from_array(&e, &[0xBBu8; 32]), &0u32);

        let record = c.get_ip(&id1);
        assert_eq!(record.ip_id, 1);
        assert_eq!(record.commitment_hash, h1);
        assert!(!record.revoked);
    }
}
