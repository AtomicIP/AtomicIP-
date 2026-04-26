#[cfg(test)]
mod tests {
    use crate::IpRecord;
    use soroban_sdk::contractclient;
    use soroban_sdk::testutils::Address as TestAddress;
    use soroban_sdk::testutils::Events;
    use soroban_sdk::{symbol_short, Address, BytesN, Env, IntoVal, TryFromVal, Vec};

    use crate::types::REVOKE_TOPIC;

    #[contractclient(name = "IpRegistryClient")]
    #[allow(dead_code)]
    pub trait IpRegistry {
        fn commit_ip(env: Env, owner: Address, commitment_hash: BytesN<32>, pow_difficulty: u32) -> u64;
        fn batch_commit_ip(env: Env, owner: Address, commitment_hashes: Vec<BytesN<32>>) -> Vec<u64>;
        fn get_ip(env: Env, ip_id: u64) -> IpRecord;
        fn verify_commitment(
            env: Env,
            ip_id: u64,
            secret: BytesN<32>,
            blinding_factor: BytesN<32>,
        ) -> bool;
        fn list_ip_by_owner(env: Env, owner: Address) -> Vec<u64>;
        fn transfer_ip(env: Env, ip_id: u64, new_owner: Address);
        fn revoke_ip(env: Env, ip_id: u64);
        fn is_ip_owner(env: Env, ip_id: u64, address: Address) -> bool;
        fn reveal_partial(
            env: Env,
            ip_id: u64,
            partial_hash: BytesN<32>,
            blinding_factor: BytesN<32>,
        ) -> bool;
        fn get_partial_disclosure(env: Env, ip_id: u64) -> Option<BytesN<32>>;
        fn validate_upgrade(env: Env, new_wasm_hash: BytesN<32>);
        fn upgrade(env: Env, new_wasm_hash: BytesN<32>);
        fn get_pow_difficulty(env: Env) -> u32;
        fn get_ip_strength(env: Env, ip_id: u64) -> u8;
        fn list_ip_by_category(env: Env, category: soroban_sdk::Bytes) -> Vec<u64>;
        fn update_ip_category(env: Env, ip_id: u64, new_category: soroban_sdk::Bytes);
        fn delegate_commitment_authority(env: Env, owner: Address, delegate_address: Address);
        fn revoke_delegation(env: Env, owner: Address, delegate_address: Address);
        fn is_delegate(env: Env, owner: Address, delegate_address: Address) -> bool;
        fn commit_ip_delegated(env: Env, owner: Address, commitment_hash: BytesN<32>, pow_difficulty: u32) -> u64;
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
        let id1 = client.commit_ip(&owner1, &commitment1, &0u32);
        let id2 = client.commit_ip(&owner2, &commitment2, &0u32);
        let id3 = client.commit_ip(&owner1, &commitment3, &0u32);

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
        let owner1_ips = client.list_ip_by_owner(&owner1);
        let owner2_ips = client.list_ip_by_owner(&owner2);

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
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        // Check events immediately after commit_ip, before any other calls.
        let all_events = env.events().all();
        assert_eq!(all_events.len(), 1);
        let event = all_events.get(0).unwrap();
        let expected_topics = (symbol_short!("ip_commit"), owner.clone()).into_val(&env);
        assert_eq!(event.1, expected_topics);
        let observed_data: (u64, u64) = TryFromVal::try_from_val(&env, &event.2).unwrap();
        assert_eq!(observed_data.0, ip_id);

        // Verify the record separately.
        let record = client.get_ip(&ip_id);
        assert_eq!(record.owner, owner);
        assert_eq!(record.commitment_hash, commitment);
        assert_eq!(record.ip_id, ip_id);
        assert_eq!(observed_data.1, record.timestamp);
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
        client.commit_ip(&owner, &zero_hash, &0u32);
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
        let ip_id = client.commit_ip(&alice, &commitment, &0u32);

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
        let ip_id = client.commit_ip(&alice, &commitment, &0u32);

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
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

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
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        assert!(!client.get_ip(&ip_id).revoked);
        client.revoke_ip(&ip_id);
        assert!(client.get_ip(&ip_id).revoked);
    }

    #[test]
    fn test_revoke_ip_emits_event() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[9u8; 32]);

        env.mock_all_auths();
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        // Clear previous events (from commit_ip)
        env.events().clear();

        client.revoke_ip(&ip_id);

        let all_events = env.events().all();
        assert_eq!(all_events.len(), 1);
        let event = all_events.get(0).unwrap();
        let expected_topics = (REVOKE_TOPIC, owner.clone()).into_val(&env);
        assert_eq!(event.1, expected_topics);
        let observed_data: (u64, u64) = TryFromVal::try_from_val(&env, &event.2).unwrap();
        assert_eq!(observed_data.0, ip_id);
        assert_eq!(observed_data.1, env.ledger().timestamp());
    }

    #[test]
    #[should_panic]
    fn test_revoke_ip_twice_panics() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        env.mock_all_auths();
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[8u8; 32]), &0u32);
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
        let id0 = client.commit_ip(&owner, &BytesN::from_array(&env, &[1u8; 32]), &0u32);
        let id1 = client.commit_ip(&owner, &BytesN::from_array(&env, &[2u8; 32]), &0u32);
        let id2 = client.commit_ip(&owner, &BytesN::from_array(&env, &[3u8; 32]), &0u32);

        assert_eq!(id0, 1);
        assert_eq!(id1, 2);
        assert_eq!(id2, 3);
    }

    /// Issue #196: verification must fail when called with the wrong secret.
    #[test]
    fn test_verify_commitment_with_invalid_secret_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let secret = BytesN::from_array(&env, &[10u8; 32]);
        let blinding = BytesN::from_array(&env, &[20u8; 32]);

        // Create an IP commitment from the valid secret + blinding pair.
        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

        let ip_id = client.commit_ip(&owner, &commitment_hash, &0u32);

        // Attempt verification with the wrong secret and assert the check fails.
        let wrong_secret = BytesN::from_array(&env, &[99u8; 32]);
        assert!(!client.verify_commitment(&ip_id, &wrong_secret, &blinding));

        // Sanity check: the original secret still verifies successfully.
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
        let id0 = client.commit_ip(&owner, &BytesN::from_array(&env, &[4u8; 32]), &0u32);
        let id1 = client.commit_ip(&owner, &BytesN::from_array(&env, &[5u8; 32]), &0u32);
        let id2 = client.commit_ip(&owner, &BytesN::from_array(&env, &[6u8; 32]), &0u32);

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
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[9u8; 32]), &0u32);

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
        let ip_id = client.commit_ip(&alice, &commitment, &0u32);

        // Alice should be the owner
        assert!(client.is_ip_owner(&ip_id, &alice));

        // Bob should not be the owner
        assert!(!client.is_ip_owner(&ip_id, &bob));

        // Non-existent IP should return false
        assert!(!client.is_ip_owner(&999u64, &alice));
    }

    // ── Partial Disclosure Tests ──────────────────────────────────────────────

    fn make_commitment(env: &Env, partial_hash: &BytesN<32>, blinding: &BytesN<32>) -> BytesN<32> {
        let mut preimage = soroban_sdk::Bytes::new(env);
        preimage.append(&soroban_sdk::Bytes::from(partial_hash.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        env.crypto().sha256(&preimage).into()
    }

    #[test]
    fn test_reveal_partial_valid_proof_returns_true_and_stores() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let partial_hash = BytesN::from_array(&env, &[0xabu8; 32]);
        let blinding = BytesN::from_array(&env, &[0xcdu8; 32]);
        let commitment = make_commitment(&env, &partial_hash, &blinding);

        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        // Valid proof: returns true
        assert!(client.reveal_partial(&ip_id, &partial_hash, &blinding));

        // Partial hash is now publicly retrievable
        assert_eq!(client.get_partial_disclosure(&ip_id), Some(partial_hash));
    }

    #[test]
    fn test_reveal_partial_wrong_blinding_returns_false() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let partial_hash = BytesN::from_array(&env, &[0x11u8; 32]);
        let blinding = BytesN::from_array(&env, &[0x22u8; 32]);
        let wrong_blinding = BytesN::from_array(&env, &[0x33u8; 32]);
        let commitment = make_commitment(&env, &partial_hash, &blinding);

        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        // Wrong blinding factor: proof fails
        assert!(!client.reveal_partial(&ip_id, &partial_hash, &wrong_blinding));

        // Nothing stored
        assert_eq!(client.get_partial_disclosure(&ip_id), None);
    }

    #[test]
    fn test_reveal_partial_wrong_partial_hash_returns_false() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let partial_hash = BytesN::from_array(&env, &[0x44u8; 32]);
        let blinding = BytesN::from_array(&env, &[0x55u8; 32]);
        let wrong_partial = BytesN::from_array(&env, &[0x66u8; 32]);
        let commitment = make_commitment(&env, &partial_hash, &blinding);

        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        assert!(!client.reveal_partial(&ip_id, &wrong_partial, &blinding));
        assert_eq!(client.get_partial_disclosure(&ip_id), None);
    }

    #[test]
    #[should_panic]
    fn test_reveal_partial_requires_owner_auth() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let attacker = <Address as TestAddress>::generate(&env);
        let partial_hash = BytesN::from_array(&env, &[0x77u8; 32]);
        let blinding = BytesN::from_array(&env, &[0x88u8; 32]);
        let commitment = make_commitment(&env, &partial_hash, &blinding);

        env.mock_all_auths();
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        // Only mock attacker's auth — must panic
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &attacker,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "reveal_partial",
                args: (ip_id, partial_hash.clone(), blinding.clone()).into_val(&env),
                sub_invokes: &[],
            },
        }]);
        client.reveal_partial(&ip_id, &partial_hash, &blinding);
    }

    #[test]
    fn test_get_partial_disclosure_none_before_reveal() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[0x99u8; 32]);
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        assert_eq!(client.get_partial_disclosure(&ip_id), None);
    }

    #[test]
    fn test_batch_commit_ip_single() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitments = Vec::from_array(&env, [BytesN::from_array(&env, &[1u8; 32])]);

        let ids = client.batch_commit_ip(&owner, &commitments);
        assert_eq!(ids.len(), 1);
        assert_eq!(ids.get(0).unwrap(), 1);
    }

    #[test]
    fn test_batch_commit_ip_five() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitments = Vec::from_array(&env, [
            BytesN::from_array(&env, &[1u8; 32]),
            BytesN::from_array(&env, &[2u8; 32]),
            BytesN::from_array(&env, &[3u8; 32]),
            BytesN::from_array(&env, &[4u8; 32]),
            BytesN::from_array(&env, &[5u8; 32]),
        ]);

        let ids = client.batch_commit_ip(&owner, &commitments);
        assert_eq!(ids.len(), 5);
        for i in 0..5 {
            assert_eq!(ids.get(i).unwrap(), (i + 1) as u64);
        }
    }

    #[test]
    fn test_batch_commit_ip_hundred() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let mut commitments = Vec::new(&env);
        for i in 0..100 {
            commitments.push_back(BytesN::from_array(&env, &[i as u8; 32]));
        }

        let ids = client.batch_commit_ip(&owner, &commitments);
        assert_eq!(ids.len(), 100);
        for i in 0..100 {
            assert_eq!(ids.get(i).unwrap(), (i + 1) as u64);
        }
    }

    #[test]
    fn test_batch_commit_ip_sequential_with_single() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);

        // Single commit
        let id1 = client.commit_ip(&owner, &BytesN::from_array(&env, &[10u8; 32]), &0u32);
        assert_eq!(id1, 1);

        // Batch commit 3
        let commitments = Vec::from_array(&env, [
            BytesN::from_array(&env, &[11u8; 32]),
            BytesN::from_array(&env, &[12u8; 32]),
            BytesN::from_array(&env, &[13u8; 32]),
        ]);
        let ids = client.batch_commit_ip(&owner, &commitments);
        assert_eq!(ids.len(), 3);
        assert_eq!(ids.get(0).unwrap(), 2);
        assert_eq!(ids.get(1).unwrap(), 3);
        assert_eq!(ids.get(2).unwrap(), 4);

        // Another single
        let id5 = client.commit_ip(&owner, &BytesN::from_array(&env, &[14u8; 32]), &0u32);
        assert_eq!(id5, 5);
    }

    #[test]
    fn test_validate_upgrade_accepts_non_zero_hash() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let valid_hash = BytesN::from_array(&env, &[1u8; 32]);
        // Should not panic
        client.validate_upgrade(&valid_hash);
    }

    #[test]
    #[should_panic]
    fn test_validate_upgrade_rejects_zero_hash() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
        client.validate_upgrade(&zero_hash);
    }

    // ── PoW Tests ─────────────────────────────────────────────────────────────

    #[test]
    fn test_get_pow_difficulty_returns_default_four() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        assert_eq!(client.get_pow_difficulty(), 4);
    }

    #[test]
    fn test_commit_ip_pow_difficulty_zero_always_passes() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        // Any non-zero hash passes when difficulty is 0
        let hash = BytesN::from_array(&env, &[0xffu8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);
        assert_eq!(ip_id, 1);
    }

    #[test]
    fn test_commit_ip_pow_difficulty_eight_accepts_leading_zero_byte() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        // First byte 0x00 = 8 leading zero bits — satisfies difficulty 8
        let mut hash_bytes = [0x01u8; 32];
        hash_bytes[0] = 0x00;
        let hash = BytesN::from_array(&env, &hash_bytes);
        let ip_id = client.commit_ip(&owner, &hash, &8u32);
        assert_eq!(ip_id, 1);
    }

    #[test]
    fn test_commit_ip_pow_difficulty_four_accepts_half_zero_nibble() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        // 0x0f = 0000_1111 — 4 leading zero bits, satisfies difficulty 4
        let mut hash_bytes = [0x01u8; 32];
        hash_bytes[0] = 0x0f;
        let hash = BytesN::from_array(&env, &hash_bytes);
        let ip_id = client.commit_ip(&owner, &hash, &4u32);
        assert_eq!(ip_id, 1);
    }

    #[test]
    #[should_panic]
    fn test_commit_ip_pow_difficulty_four_rejects_insufficient_leading_zeros() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        // 0x1f = 0001_1111 — only 3 leading zero bits, fails difficulty 4
        let mut hash_bytes = [0x01u8; 32];
        hash_bytes[0] = 0x1f;
        let hash = BytesN::from_array(&env, &hash_bytes);
        client.commit_ip(&owner, &hash, &4u32);
    }

    #[test]
    #[should_panic]
    fn test_commit_ip_pow_difficulty_one_rejects_high_bit_set() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        // 0x80 = 1000_0000 — high bit set, fails difficulty 1
        let mut hash_bytes = [0x01u8; 32];
        hash_bytes[0] = 0x80;
        let hash = BytesN::from_array(&env, &hash_bytes);
        client.commit_ip(&owner, &hash, &1u32);
    }

    // ── Tests for Issue #335: IP Commitment Strength Scoring ──────────────────

    #[test]
    fn test_get_ip_strength() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);

        let ip_id = client.commit_ip(&owner, &hash, &4u32);
        let strength = client.get_ip_strength(&ip_id);

        // Strength should be calculated based on secret length (32) and PoW difficulty (4)
        // Formula: min(100, (32 * 2) + (4 * 3)) = min(100, 64 + 12) = 76
        assert_eq!(strength, 76u8);
    }

    #[test]
    fn test_get_ip_strength_max_capped_at_100() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);

        let ip_id = client.commit_ip(&owner, &hash, &20u32);
        let strength = client.get_ip_strength(&ip_id);

        // Strength should be capped at 100
        assert_eq!(strength, 100u8);
    }

    // ── Tests for Issue #336: IP Compartmentalization by Category ──────────────

    #[test]
    fn test_update_ip_category() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);

        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        // Update category
        let category = soroban_sdk::Bytes::from_slice(&env, b"software");
        client.update_ip_category(&ip_id, &category);

        let record = client.get_ip(&ip_id);
        assert_eq!(record.category, category);
    }

    #[test]
    fn test_list_ip_by_category() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let hash1 = BytesN::from_array(&env, &[1u8; 32]);
        let hash2 = BytesN::from_array(&env, &[2u8; 32]);

        let ip_id1 = client.commit_ip(&owner, &hash1, &0u32);
        let ip_id2 = client.commit_ip(&owner, &hash2, &0u32);

        let category = soroban_sdk::Bytes::from_slice(&env, b"software");
        client.update_ip_category(&ip_id1, &category);
        client.update_ip_category(&ip_id2, &category);

        let ids = client.list_ip_by_category(&category);
        assert_eq!(ids.len(), 2);
        assert_eq!(ids.get(0).unwrap(), ip_id1);
        assert_eq!(ids.get(1).unwrap(), ip_id2);
    }

    // ── Tests for Issue #338: IP Commitment Delegation ────────────────────────

    #[test]
    fn test_delegate_commitment_authority() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let delegate = <Address as TestAddress>::generate(&env);

        client.delegate_commitment_authority(&owner, &delegate);

        let is_delegate = client.is_delegate(&owner, &delegate);
        assert!(is_delegate);
    }

    #[test]
    fn test_revoke_delegation() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let delegate = <Address as TestAddress>::generate(&env);

        client.delegate_commitment_authority(&owner, &delegate);
        assert!(client.is_delegate(&owner, &delegate));

        client.revoke_delegation(&owner, &delegate);
        assert!(!client.is_delegate(&owner, &delegate));
    }

    #[test]
    fn test_commit_ip_delegated() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let delegate = <Address as TestAddress>::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);

        client.delegate_commitment_authority(&owner, &delegate);
        let ip_id = client.commit_ip_delegated(&owner, &hash, &0u32);

        let record = client.get_ip(&ip_id);
        assert_eq!(record.owner, owner);
        assert_eq!(record.commitment_hash, hash);
    }
}
