#[cfg(test)]
mod tests {
    use crate::IpRecord;
    use soroban_sdk::contractclient;
    use soroban_sdk::testutils::Address as TestAddress;
    use soroban_sdk::testutils::Events;
    use soroban_sdk::{symbol_short, Address, BytesN, Env, IntoVal, TryFromVal, Vec};

    #[contractclient(name = "IpRegistryClient")]
    #[allow(dead_code)]
    pub trait IpRegistry {
        fn commit_ip(env: Env, owner: Address, commitment_hash: BytesN<32>) -> u64;
        fn get_ip(env: Env, ip_id: u64) -> IpRecord;
        fn verify_commitment(env: Env, ip_id: u64, secret: BytesN<32>, blinding_factor: BytesN<32>) -> bool;
        fn list_ip_by_owner(env: Env, owner: Address) -> Vec<u64>;
        fn transfer_ip(env: Env, ip_id: u64, new_owner: Address);
        fn revoke_ip(env: Env, ip_id: u64);
        fn is_ip_owner(env: Env, ip_id: u64, address: Address) -> bool;
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

        // Assert IDs are sequential: 1, 2, 3 (first ID is 1, not 0)
        assert_eq!(id1, 1, "First commit should return ID 1");
        assert_eq!(id2, 2, "Second commit should return ID 2");
        assert_eq!(id3, 3, "Third commit should return ID 3");

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
        let owner1_ips = client.list_ip_by_owner(&owner1).unwrap();
        let owner2_ips = client.list_ip_by_owner(&owner2).unwrap();

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
    #[should_panic]
    fn test_commit_ip_zero_hash_rejected() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        env.mock_all_auths();

        // All-zero hash has no cryptographic value — must panic with ContractError::ZeroCommitmentHash (code 2)
        let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
        client.commit_ip(&owner, &zero_hash);
    }

    #[test]
    #[should_panic]
    fn test_get_ip_nonexistent_returns_structured_error() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        // ID 999 was never committed — must panic with ContractError::IpNotFound (code 1)
        client.get_ip(&999u64);
    }

    #[test]
    fn test_transfer_ip_updates_owner_and_indexes() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let alice = <Address as TestAddress>::generate(&env);
        let bob = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[5u8; 32]);

        env.mock_all_auths();
        let ip_id = client.commit_ip(&alice, &commitment);

        client.transfer_ip(&ip_id, &bob);

        // Record owner updated
        let record = client.get_ip(&ip_id);
        assert_eq!(record.owner, bob);

        // Old owner index no longer contains ip_id
        let alice_ips = client.list_ip_by_owner(&alice);
        assert!(!alice_ips.iter().any(|x| x == ip_id));

        // New owner index contains ip_id
        let bob_ips = client.list_ip_by_owner(&bob);
        assert!(bob_ips.iter().any(|x| x == ip_id));
    }

    #[test]
    #[should_panic]
    fn test_transfer_ip_requires_owner_auth() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let alice = <Address as TestAddress>::generate(&env);
        let bob = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[6u8; 32]);

        env.mock_all_auths();
        let ip_id = client.commit_ip(&alice, &commitment);

        // Only mock bob's auth — alice's auth is not present, so transfer must panic
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &bob,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "transfer_ip",
                args: (ip_id, bob.clone()).into_val(&env),
                sub_invokes: &[],
            },
        }]);
        client.transfer_ip(&ip_id, &bob);
    }

    #[test]
    #[should_panic]
    fn test_transfer_ip_nonexistent_panics() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let bob = <Address as TestAddress>::generate(&env);
        env.mock_all_auths();
        client.transfer_ip(&999u64, &bob);
    }

    #[test]
    fn test_list_ip_by_owner_unknown_returns_empty() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let unknown_owner = <Address as TestAddress>::generate(&env);
        env.mock_all_auths();

        // Commit an IP for owner
        let commitment = BytesN::from_array(&env, &[1u8; 32]);
        let ip_id = client.commit_ip(&owner, &commitment);

        // Unknown owner returns empty Vec; known owner returns Vec with IPs.
        let unknown_ips = client.list_ip_by_owner(&unknown_owner);
        assert_eq!(unknown_ips.len(), 0);

        let owner_ips = client.list_ip_by_owner(&owner);
        assert_eq!(owner_ips.len(), 1);
        assert_eq!(owner_ips.get(0).unwrap(), ip_id);
    }

    #[test]
    fn test_revoke_ip_marks_record_revoked() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[7u8; 32]);

        env.mock_all_auths();
        let ip_id = client.commit_ip(&owner, &commitment);

        assert!(!client.get_ip(&ip_id).revoked);
        client.revoke_ip(&ip_id);
        assert!(client.get_ip(&ip_id).revoked);
    }

    #[test]
    #[should_panic]
    fn test_revoke_ip_twice_panics() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        env.mock_all_auths();
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[8u8; 32]));
        client.revoke_ip(&ip_id);
        client.revoke_ip(&ip_id); // must panic with IpAlreadyRevoked (code 4)
    }

    /// Issue: Verify commit_ip assigns IDs sequentially (1, 2, 3).
    #[test]
    fn test_sequential_ip_ids() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let id0 = client.commit_ip(&owner, &BytesN::from_array(&env, &[1u8; 32]));
        let id1 = client.commit_ip(&owner, &BytesN::from_array(&env, &[2u8; 32]));
        let id2 = client.commit_ip(&owner, &BytesN::from_array(&env, &[3u8; 32]));

        assert_eq!(id0, 1);
        assert_eq!(id1, 2);
        assert_eq!(id2, 3);
    }

    /// Issue: verify_commitment returns false for a wrong secret.
    #[test]
    fn test_wrong_secret_returns_false() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let secret = BytesN::from_array(&env, &[10u8; 32]);
        let blinding = BytesN::from_array(&env, &[20u8; 32]);

        // Build commitment_hash = sha256(secret || blinding)
        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

        let ip_id = client.commit_ip(&owner, &commitment_hash);

        // Wrong secret — must return false
        let wrong_secret = BytesN::from_array(&env, &[99u8; 32]);
        assert!(!client.verify_commitment(&ip_id, &wrong_secret, &blinding));
        // Correct secret — must return true
        assert!(client.verify_commitment(&ip_id, &secret, &blinding));
    }

    /// Issue: list_ip_by_owner returns all IDs committed by an owner in order.
    #[test]
    fn test_list_ip_by_owner_returns_all_ids() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let id0 = client.commit_ip(&owner, &BytesN::from_array(&env, &[4u8; 32]));
        let id1 = client.commit_ip(&owner, &BytesN::from_array(&env, &[5u8; 32]));
        let id2 = client.commit_ip(&owner, &BytesN::from_array(&env, &[6u8; 32]));

        let ids = client.list_ip_by_owner(&owner);
        assert_eq!(ids.len(), 3);
        assert_eq!(ids.get(0).unwrap(), id0);
        assert_eq!(ids.get(1).unwrap(), id1);
        assert_eq!(ids.get(2).unwrap(), id2);
    }

    #[test]
    #[should_panic]
    fn test_revoke_ip_requires_owner_auth() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let attacker = <Address as TestAddress>::generate(&env);
        env.mock_all_auths();
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[9u8; 32]));

        // Only mock attacker's auth — owner's auth is absent, must panic
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &attacker,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "revoke_ip",
                args: (ip_id,).into_val(&env),
                sub_invokes: &[],
            },
        }]);
        client.revoke_ip(&ip_id);
    }

    #[test]
    fn test_is_ip_owner() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let alice = <Address as TestAddress>::generate(&env);
        let bob = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[10u8; 32]);

        env.mock_all_auths();
        let ip_id = client.commit_ip(&alice, &commitment);

        // Alice should be the owner
        assert!(client.is_ip_owner(&ip_id, &alice));

        // Bob should not be the owner
        assert!(!client.is_ip_owner(&ip_id, &bob));

        // Non-existent IP should return false
        assert!(!client.is_ip_owner(&999u64, &alice));
    }
}
