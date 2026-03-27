#[cfg(test)]
mod tests {
    use crate::IpRecord;
    use soroban_sdk::contractclient;
    use soroban_sdk::testutils::{Address as TestAddress, Events as _};
    use soroban_sdk::{symbol_short, Address, BytesN, Env, IntoVal, TryFromVal, Vec};

    #[contractclient(name = "IpRegistryClient")]
    #[allow(dead_code)]
    pub trait IpRegistry {
        fn commit_ip(env: Env, owner: Address, commitment_hash: BytesN<32>) -> u64;
        fn get_ip(env: Env, ip_id: u64) -> IpRecord;
        fn list_ip_by_owner(env: Env, owner: Address) -> Option<Vec<u64>>;
    }

    #[test]
    fn test_commit_ip_sequential_ids() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        // Create test addresses using the test environment
        let owner1 = <Address as TestAddress>::generate(&env);
        let owner2 = <Address as TestAddress>::generate(&env);

        // Create test commitment hashes
        let commitment1 = BytesN::from_array(&env, &[1u8; 32]);
        let commitment2 = BytesN::from_array(&env, &[2u8; 32]);
        let commitment3 = BytesN::from_array(&env, &[3u8; 32]);

        // Call commit_ip three times with proper authentication
        env.mock_all_auths();
        let id1 = client.commit_ip(&owner1, &commitment1);
        let id2 = client.commit_ip(&owner2, &commitment2);
        let id3 = client.commit_ip(&owner1, &commitment3);

        // Assert IDs are sequential: 0, 1, 2
        assert_eq!(id1, 0, "First commit should return ID 0");
        assert_eq!(id2, 1, "Second commit should return ID 1");
        assert_eq!(id3, 2, "Third commit should return ID 2");

        // Verify the records are stored correctly
        let record1 = client.get_ip(&id1);
        let record2 = client.get_ip(&id2);
        let record3 = client.get_ip(&id3);

        assert_eq!(record1.owner, owner1);
        assert_eq!(record1.commitment_hash, commitment1);

        assert_eq!(record2.owner, owner2);
        assert_eq!(record2.commitment_hash, commitment2);

        assert_eq!(record3.owner, owner1);
        assert_eq!(record3.commitment_hash, commitment3);

        // Verify owner index is correct
        let owner1_ips = client
            .list_ip_by_owner(&owner1)
            .expect("owner1 should have committed IPs");
        let owner2_ips = client
            .list_ip_by_owner(&owner2)
            .expect("owner2 should have committed IPs");

        assert_eq!(owner1_ips.len(), 2);
        assert_eq!(owner2_ips.len(), 1);
        assert_eq!(owner1_ips.get(0).unwrap(), id1);
        assert_eq!(owner1_ips.get(1).unwrap(), id3);
        assert_eq!(owner2_ips.get(0).unwrap(), id2);
    }

    #[test]
    fn test_commit_ip_emits_event() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[42u8; 32]);

        env.mock_all_auths();

        // Call commit_ip which should emit an event
        let ip_id = client.commit_ip(&owner, &commitment);

        // Verify the event payload and topic.
        let record = client.get_ip(&ip_id);

        let all_events = env.events().all();
        assert_eq!(all_events.len(), 1);
        let event = all_events.get(0).unwrap();
        let expected_topics = (symbol_short!("ip_commit"), owner.clone()).into_val(&env);
        let expected_data = (ip_id, record.timestamp);
        assert_eq!(event.1, expected_topics);
        let observed_data: (u64, u64) = TryFromVal::try_from_val(&env, &event.2).unwrap();
        assert_eq!(observed_data, expected_data);

        assert_eq!(record.owner, owner);
        assert_eq!(record.commitment_hash, commitment);
        assert_eq!(record.ip_id, ip_id);
    }

    #[test]
    fn test_list_ip_by_owner_unknown_returns_none() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let unknown_owner = <Address as TestAddress>::generate(&env);
        env.mock_all_auths();

        // Commit an IP for owner
        let commitment = BytesN::from_array(&env, &[1u8; 32]);
        let ip_id = client.commit_ip(&owner, &commitment);

        // Unknown owner returns None; known owner returns Some(Vec).
        let unknown_ips = client.list_ip_by_owner(&unknown_owner);
        assert_eq!(unknown_ips, None);

        let owner_ips = client
            .list_ip_by_owner(&owner)
            .expect("owner should have committed IPs");
        assert_eq!(owner_ips.len(), 1);
        assert_eq!(owner_ips.get(0).unwrap(), ip_id);
    }
}
