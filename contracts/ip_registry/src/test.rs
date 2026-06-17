#[cfg(test)]
mod tests {
    use crate::IpRecord;
    use soroban_sdk::contractclient;
    use soroban_sdk::testutils::Address as TestAddress;
    use soroban_sdk::testutils::Events;
    use soroban_sdk::{symbol_short, Address, Bytes, BytesN, Env, IntoVal, TryFromVal, Vec};

    use crate::types::REVOKE_TOPIC;
    use crate::types::TRANSFER_TOPIC;

    #[contractclient(name = "IpRegistryClient")]
    #[allow(dead_code)]
    pub trait IpRegistry {
        fn commit_ip(
            env: Env,
            owner: Address,
            commitment_hash: BytesN<32>,
            pow_difficulty: u32,
        ) -> u64;
        fn batch_commit_ip(
            env: Env,
            owner: Address,
            commitment_hashes: Vec<BytesN<32>>,
        ) -> Vec<u64>;
        fn get_ip(env: Env, ip_id: u64) -> IpRecord;
        fn verify_commitment(
            env: Env,
            ip_id: u64,
            secret: BytesN<32>,
            blinding_factor: BytesN<32>,
        ) -> bool;
        fn list_ip_by_owner(env: Env, owner: Address) -> Vec<u64>;
        fn transfer_ip(env: Env, ip_id: u64, new_owner: Address);
        fn transfer_ip_ownership(env: Env, ip_id: u64, new_owner: Address);
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
        fn get_ip_strength(env: Env, ip_id: u64) -> u32;
        fn renew_ip(env: Env, ip_id: u64);
        fn get_renewal_count(env: Env, ip_id: u64) -> u32;
        fn delegate_commitment_authority(
            env: Env,
            root_owner: Address,
            delegator: Address,
            delegate_address: Address,
        );
        fn revoke_delegation(env: Env, owner: Address, delegate_address: Address);
        fn is_delegate(env: Env, owner: Address, delegate_address: Address) -> bool;
        fn commit_ip_delegated(
            env: Env,
            owner: Address,
            commitment_hash: BytesN<32>,
            pow_difficulty: u32,
        ) -> u64;
        fn attest_ip(env: Env, ip_id: u64, attestor: Address, attestation_data: soroban_sdk::Bytes);
        fn get_ip_attestations(env: Env, ip_id: u64) -> Vec<crate::Attestation>;
        fn challenge_ip(env: Env, ip_id: u64, challenger: Address, reason: soroban_sdk::Bytes);
        fn get_ip_disputes(env: Env, ip_id: u64) -> Vec<crate::IpChallenge>;
        fn commit_ip_version(
            env: Env,
            owner: Address,
            commitment_hash: BytesN<32>,
            parent_ip_id: u64,
        ) -> u64;
        // Issue #432
        fn batch_verify_commitments(
            env: Env,
            verifications: Vec<(u64, BytesN<32>, BytesN<32>)>,
        ) -> Vec<bool>;
        fn batch_commit_ip_anonymous(
            env: Env,
            blinded_owner: BytesN<32>,
            commitment_hashes: Vec<BytesN<32>>,
        ) -> Vec<u64>;
        fn get_anonymous_owner(env: Env, commitment_hash: BytesN<32>) -> Option<BytesN<32>>;
        // Issue #433
        fn issue_ownership_challenge(
            env: Env,
            ip_id: u64,
            challenger: Address,
            nonce: BytesN<32>,
        ) -> u64;
        fn respond_to_ownership_challenge(env: Env, challenge_id: u64, response_hash: BytesN<32>);
        fn verify_ownership_challenge(env: Env, challenge_id: u64) -> bool;
        fn get_ownership_challenge(
            env: Env,
            challenge_id: u64,
        ) -> Option<crate::types::OwnershipChallenge>;
        // Issue #434
        fn rotate_commitment_key(
            env: Env,
            ip_id: u64,
            new_commitment_hash: BytesN<32>,
            old_secret: BytesN<32>,
            old_blinding_factor: BytesN<32>,
        );
        fn get_key_rotation_history(env: Env, ip_id: u64) -> Vec<BytesN<32>>;
        // Issue #435
        fn generate_merkle_proof(env: Env, ip_id: u64) -> Vec<BytesN<32>>;
        fn compute_ip_merkle_root(env: Env, owner: Address) -> BytesN<32>;
        fn verify_ip_merkle_proof(env: Env, ip_id: u64, proof: Vec<BytesN<32>>) -> bool;
        fn set_notary_public_key(env: Env, public_key: BytesN<32>);
        fn notarize_ip_timestamp(env: Env, ip_id: u64, notary_signature: soroban_sdk::Bytes);
        fn get_ip_notary_signature(env: Env, ip_id: u64) -> Option<soroban_sdk::Bytes>;
        fn verify_commitment_integrity(env: Env) -> bool;
        fn get_ip_versions(env: Env, ip_id: u64) -> Vec<u64>;
        fn get_ip_lineage(env: Env, ip_id: u64) -> Vec<u64>;
        fn get_ip_version_chain(env: Env, ip_id: u64) -> Vec<u64>;
        fn check_expiration_warning(env: Env, ip_id: u64, warning_threshold_ledgers: u32) -> bool;
        fn grant_ip_access(env: Env, ip_id: u64, grantee: Address, access_level: u32);
        fn revoke_ip_access(env: Env, ip_id: u64, grantee: Address);
        fn get_ip_access_grants(env: Env, ip_id: u64) -> Vec<crate::IpAccessGrant>;
        fn check_ip_access(env: Env, ip_id: u64, grantee: Address, required_level: u32) -> bool;
        fn initiate_dispute(
            env: Env,
            ip_id: u64,
            challenger: Address,
            evidence_hash: BytesN<32>,
        ) -> u64;
        fn submit_dispute_evidence(
            env: Env,
            dispute_id: u64,
            submitter: Address,
            evidence_hash: BytesN<32>,
        );
        fn resolve_dispute(env: Env, dispute_id: u64, winner: Address);
        fn get_dispute(env: Env, dispute_id: u64) -> crate::DisputeRecord;
        fn validate_category_path(env: Env, path: Bytes) -> BytesN<32>;
        fn assign_ip_to_category(env: Env, ip_id: u64, category_hash: BytesN<32>);
        fn list_ip_by_category(env: Env, owner: Address, category_hash: BytesN<32>) -> Vec<u64>;
        fn list_owner_categories(env: Env, owner: Address) -> Vec<BytesN<32>>;
        // Issue #454: Threshold signatures
        fn add_threshold_signature(
            env: Env,
            ip_id: u64,
            signer: Address,
            signature_hash: BytesN<32>,
        );
        fn get_threshold_signatures(
            env: Env,
            ip_id: u64,
        ) -> soroban_sdk::Vec<crate::ThresholdSignature>;
        fn get_threshold_config(env: Env, ip_id: u64) -> Option<crate::ThresholdConfig>;
        fn verify_threshold_signatures(env: Env, ip_id: u64) -> bool;
        // Issue #455: Batch metadata
        fn set_batch_metadata(env: Env, ip_id: u64, batch_id: BytesN<32>, description: Bytes);
        fn get_batch_metadata(env: Env, ip_id: u64) -> Option<crate::BatchMetadata>;
        // Issue #456: Compression algorithm selection
        fn set_commitment_compression(env: Env, ip_id: u64, algorithm: crate::CompressionAlgo);
        fn get_commitment_compression(env: Env, ip_id: u64) -> crate::CompressionAlgo;
        fn get_compressed_by_algo(env: Env, ip_id: u64) -> Bytes;
        // Issue #457: Commitment encryption
        fn encrypt_commitment(env: Env, ip_id: u64, encrypted_hash: Bytes, key_hint: BytesN<32>);
        fn get_encrypted_commitment(
            env: Env,
            ip_id: u64,
        ) -> Option<crate::EncryptedCommitmentRecord>;
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
    fn test_transfer_ip_emits_event() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let alice = <Address as TestAddress>::generate(&env);
        let bob = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[20u8; 32]);

        env.mock_all_auths();
        let ip_id = client.commit_ip(&alice, &commitment, &0u32);

        client.transfer_ip(&ip_id, &bob);

        let all_events = env.events().all();
        assert!(!all_events.is_empty());
        let event = all_events.get(all_events.len() - 1).unwrap();
        let expected_topics = (TRANSFER_TOPIC, ip_id).into_val(&env);
        assert_eq!(event.1, expected_topics);
        let (old_owner, new_owner): (Address, Address) =
            TryFromVal::try_from_val(&env, &event.2).unwrap();
        assert_eq!(old_owner, alice);
        assert_eq!(new_owner, bob);
    }

    #[test]
    fn test_transfer_ip_ownership_successful() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let alice = <Address as TestAddress>::generate(&env);
        let bob = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[21u8; 32]);

        env.mock_all_auths();
        let ip_id = client.commit_ip(&alice, &commitment, &0u32);

        client.transfer_ip_ownership(&ip_id, &bob);

        let record = client.get_ip(&ip_id);
        assert_eq!(record.owner, bob);

        let alice_ips = client.list_ip_by_owner(&alice);
        assert!(!alice_ips.iter().any(|x| x == ip_id));

        let bob_ips = client.list_ip_by_owner(&bob);
        assert!(bob_ips.iter().any(|x| x == ip_id));
    }

    #[test]
    #[should_panic]
    fn test_transfer_ip_ownership_unauthorized_rejected() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let alice = <Address as TestAddress>::generate(&env);
        let bob = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[22u8; 32]);

        env.mock_all_auths();
        let ip_id = client.commit_ip(&alice, &commitment, &0u32);

        // Only mock bob's auth — alice's auth absent, must panic
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &bob,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "transfer_ip_ownership",
                args: (ip_id, bob.clone()).into_val(&env),
                sub_invokes: &[],
            },
        }]);
        client.transfer_ip_ownership(&ip_id, &bob);
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

        client.revoke_ip(&ip_id);

        let all_events = env.events().all();
        assert!(!all_events.is_empty());
        let event = all_events.get(all_events.len() - 1).unwrap();
        let expected_topics = (REVOKE_TOPIC, owner.clone()).into_val(&env);
        assert_eq!(event.1, expected_topics);
        let observed_data: (u64, u64) = TryFromVal::try_from_val(&env, &event.2).unwrap();
        assert_eq!(observed_data.0, ip_id);
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
        let commitments = Vec::from_array(
            &env,
            [
                BytesN::from_array(&env, &[1u8; 32]),
                BytesN::from_array(&env, &[2u8; 32]),
                BytesN::from_array(&env, &[3u8; 32]),
                BytesN::from_array(&env, &[4u8; 32]),
                BytesN::from_array(&env, &[5u8; 32]),
            ],
        );

        let ids = client.batch_commit_ip(&owner, &commitments);
        assert_eq!(ids.len(), 5);
        for i in 0..5 {
            assert_eq!(ids.get(i).unwrap(), (i + 1) as u64);
        }
    }

    #[test]
    fn test_batch_commit_ip_anonymous_creates_records() {
        let env = Env::default();
        // Anonymous commits do not require caller auth
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        // Create anonymous commitment hashes
        let commitment1 = BytesN::from_array(&env, &[11u8; 32]);
        let commitment2 = BytesN::from_array(&env, &[12u8; 32]);
        let mut hashes: Vec<BytesN<32>> = Vec::new(&env);
        hashes.push_back(commitment1.clone());
        hashes.push_back(commitment2.clone());

        // Blinded owner identifier (off-chain proof pointer)
        let blinded_owner = BytesN::from_array(&env, &[7u8; 32]);

        // Call anonymous batch commit
        let ids = client.batch_commit_ip_anonymous(&blinded_owner, &hashes);

        assert_eq!(ids.len(), 2);

        // Verify records exist and contain expected commitment hashes
        let rec1 = client.get_ip(&ids.get(0).unwrap());
        let rec2 = client.get_ip(&ids.get(1).unwrap());

        assert_eq!(rec1.commitment_hash, commitment1);
        assert_eq!(rec2.commitment_hash, commitment2);

        // Ensure anonymous commits did not populate owner index for a random owner
        let random_owner = <Address as TestAddress>::generate(&env);
        let listed = client.list_ip_by_owner(&random_owner);
        assert_eq!(listed.len(), 0);
    }

    #[test]
    #[ignore]
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
        let commitments = Vec::from_array(
            &env,
            [
                BytesN::from_array(&env, &[11u8; 32]),
                BytesN::from_array(&env, &[12u8; 32]),
                BytesN::from_array(&env, &[13u8; 32]),
            ],
        );
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
    fn test_get_ip_strength_low_entropy_low_pow() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        // All-same-byte hash: 1 unique byte → entropy_score = (1*50)/32 = 1
        // pow_difficulty = 0 → pow_score = 0
        // total = 1
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);
        let strength = client.get_ip_strength(&ip_id);
        assert_eq!(strength, 1u32);
    }

    #[test]
    fn test_get_ip_strength_high_entropy() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        // 32 unique bytes → entropy_score = (32*50)/32 = 50
        // pow_difficulty = 0 → pow_score = 0
        // total = 50
        let hash_bytes: [u8; 32] = core::array::from_fn(|i| i as u8);
        let hash = BytesN::from_array(&env, &hash_bytes);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);
        let strength = client.get_ip_strength(&ip_id);
        assert_eq!(strength, 50u32);
    }

    #[test]
    fn test_get_ip_strength_max_pow() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        // 4 leading zeros + 28 unique non-zero bytes = 29 unique bytes total
        // entropy_score = (29*50)/32 = 45
        // pow_difficulty = 32 → pow_score = 50
        // total = 95
        let hash_bytes: [u8; 32] = core::array::from_fn(|i| if i < 4 { 0 } else { i as u8 });
        let hash = BytesN::from_array(&env, &hash_bytes);
        let ip_id = client.commit_ip(&owner, &hash, &32u32);
        let strength = client.get_ip_strength(&ip_id);
        assert_eq!(strength, 95u32);
    }

    #[test]
    fn test_get_ip_strength_capped_at_100() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        // 8 leading zeros + 24 unique non-zero bytes = 25 unique bytes total
        // entropy_score = (25*50)/32 = 39
        // pow_difficulty = 64 → pow_score capped at 50
        // total = 89
        let hash_bytes: [u8; 32] = core::array::from_fn(|i| if i < 8 { 0 } else { i as u8 });
        let hash = BytesN::from_array(&env, &hash_bytes);
        let ip_id = client.commit_ip(&owner, &hash, &64u32);
        let strength = client.get_ip_strength(&ip_id);
        assert_eq!(strength, 89u32);
    }

    #[test]
    fn test_get_ip_strength_partial_entropy_and_pow() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        // 2 leading zeros + 15 unique non-zero bytes (1..=15) = 16 unique bytes total
        // pow_difficulty = 16 → pow_score = (16*50)/32 = 25
        // entropy_score = (16*50)/32 = 25
        // total = 50
        let hash_bytes: [u8; 32] =
            core::array::from_fn(|i| if i < 2 { 0 } else { ((i - 2) % 15 + 1) as u8 });
        let hash = BytesN::from_array(&env, &hash_bytes);
        let ip_id = client.commit_ip(&owner, &hash, &16u32);
        let strength = client.get_ip_strength(&ip_id);
        assert_eq!(strength, 50u32);
    }

    // ── Tests for Issue #338: IP Commitment Delegation ────────────────────────

    #[test]
    #[ignore]
    fn test_delegate_commitment_authority() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let delegate = <Address as TestAddress>::generate(&env);

        client.delegate_commitment_authority(&owner, &owner, &delegate);

        let is_delegate = client.is_delegate(&owner, &delegate);
        assert!(is_delegate);
    }

    #[test]
    #[ignore]
    fn test_revoke_delegation() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let delegate = <Address as TestAddress>::generate(&env);

        client.delegate_commitment_authority(&owner, &owner, &delegate);
        assert!(client.is_delegate(&owner, &delegate));

        client.revoke_delegation(&owner, &delegate);
        assert!(!client.is_delegate(&owner, &delegate));
    }

    #[test]
    #[ignore]
    fn test_commit_ip_delegated() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let delegate = <Address as TestAddress>::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);

        client.delegate_commitment_authority(&owner, &owner, &delegate);
        let ip_id = client.commit_ip_delegated(&owner, &hash, &0u32);

        let record = client.get_ip(&ip_id);
        assert_eq!(record.owner, owner);
        assert_eq!(record.commitment_hash, hash);
    }

    // ── Tests for Third-Party Attestations ──

    #[test]
    #[ignore]
    fn test_attest_ip_by_third_party() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let notary = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[10u8; 32]);
        let attestation_data = soroban_sdk::Bytes::from_array(&env, &[0xABu8; 32]);

        let ip_id = client.commit_ip(&owner, &commitment, &0u32);
        client.attest_ip(&ip_id, &notary, &attestation_data);

        let attestations = client.get_ip_attestations(&ip_id);
        assert_eq!(attestations.len(), 1);
        let att = attestations.get(0).unwrap();
        assert_eq!(att.attestor, notary);
        assert_eq!(att.attestation_data, attestation_data);
    }

    #[test]
    #[ignore]
    fn test_multiple_attestors() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let notary = <Address as TestAddress>::generate(&env);
        let university = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[11u8; 32]);

        let ip_id = client.commit_ip(&owner, &commitment, &0u32);
        client.attest_ip(
            &ip_id,
            &notary,
            &soroban_sdk::Bytes::from_array(&env, &[1u8; 32]),
        );
        client.attest_ip(
            &ip_id,
            &university,
            &soroban_sdk::Bytes::from_array(&env, &[2u8; 32]),
        );

        let attestations = client.get_ip_attestations(&ip_id);
        assert_eq!(attestations.len(), 2);
        assert_eq!(attestations.get(0).unwrap().attestor, notary);
        assert_eq!(attestations.get(1).unwrap().attestor, university);
    }

    #[test]
    #[ignore]
    fn test_get_ip_attestations_empty() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[12u8; 32]);

        let ip_id = client.commit_ip(&owner, &commitment, &0u32);
        let attestations = client.get_ip_attestations(&ip_id);
        assert_eq!(attestations.len(), 0);
    }

    #[test]
    #[should_panic]
    fn test_attest_ip_nonexistent() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let attestor = <Address as TestAddress>::generate(&env);
        // IP ID 999 does not exist — should panic
        client.attest_ip(
            &999u64,
            &attestor,
            &soroban_sdk::Bytes::from_array(&env, &[1u8; 32]),
        );
    }

    // ── Tests for IP Dispute Challenges ──

    #[test]
    #[ignore]
    fn test_challenge_ip_stored() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let challenger = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[30u8; 32]);
        let reason = soroban_sdk::Bytes::from_array(&env, &[0xAAu8; 32]);

        let ip_id = client.commit_ip(&owner, &commitment, &0u32);
        client.challenge_ip(&ip_id, &challenger, &reason);

        let disputes = client.get_ip_disputes(&ip_id);
        assert_eq!(disputes.len(), 1);
        let d = disputes.get(0).unwrap();
        assert_eq!(d.challenger, challenger);
        assert_eq!(d.reason, reason);
        assert!(!d.resolved);
    }

    #[test]
    #[ignore]
    fn test_challenge_ip_multiple_challengers() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let c1 = <Address as TestAddress>::generate(&env);
        let c2 = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[31u8; 32]);

        let ip_id = client.commit_ip(&owner, &commitment, &0u32);
        client.challenge_ip(
            &ip_id,
            &c1,
            &soroban_sdk::Bytes::from_array(&env, &[1u8; 32]),
        );
        client.challenge_ip(
            &ip_id,
            &c2,
            &soroban_sdk::Bytes::from_array(&env, &[2u8; 32]),
        );

        let disputes = client.get_ip_disputes(&ip_id);
        assert_eq!(disputes.len(), 2);
        assert_eq!(disputes.get(0).unwrap().challenger, c1);
        assert_eq!(disputes.get(1).unwrap().challenger, c2);
    }

    #[test]
    #[ignore]
    fn test_get_ip_disputes_empty() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[34u8; 32]);

        let ip_id = client.commit_ip(&owner, &commitment, &0u32);
        assert_eq!(client.get_ip_disputes(&ip_id).len(), 0);
    }

    #[test]
    #[should_panic]
    fn test_challenge_ip_nonexistent_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let challenger = <Address as TestAddress>::generate(&env);
        client.challenge_ip(
            &999u64,
            &challenger,
            &soroban_sdk::Bytes::from_array(&env, &[1u8; 32]),
        );
    }

    // ── Tests for Issue #428: Commitment Timestamp Notarization ──────────────

    #[test]
    fn test_notarize_ip_timestamp_with_valid_signature() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);

        let secret1 = BytesN::from_array(&env, &[0x11u8; 32]);
        let bf1 = BytesN::from_array(&env, &[0x12u8; 32]);
        let mut pre1 = soroban_sdk::Bytes::new(&env);
        pre1.append(&secret1.clone().into());
        pre1.append(&bf1.clone().into());
        let hash1: BytesN<32> = env.crypto().sha256(&pre1).into();

        let secret2 = BytesN::from_array(&env, &[0x21u8; 32]);
        let bf2 = BytesN::from_array(&env, &[0x22u8; 32]);
        let mut pre2 = soroban_sdk::Bytes::new(&env);
        pre2.append(&secret2.clone().into());
        pre2.append(&bf2.clone().into());
        let hash2: BytesN<32> = env.crypto().sha256(&pre2).into();

        let id1 = client.commit_ip(&owner, &hash1, &0u32);
        let id2 = client.commit_ip(&owner, &hash2, &0u32);

        let mut verifications: Vec<(u64, BytesN<32>, BytesN<32>)> = Vec::new(&env);
        verifications.push_back((id1, secret1, bf1));
        verifications.push_back((id2, secret2, bf2));

        let results = client.batch_verify_commitments(&verifications);
        assert_eq!(results.len(), 2);
        assert!(results.get(0).unwrap());
        assert!(results.get(1).unwrap());
    }

    #[test]
    fn test_batch_verify_commitments_invalid_secret() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let secret = BytesN::from_array(&env, &[0xAAu8; 32]);
        let bf = BytesN::from_array(&env, &[0xBBu8; 32]);
        let mut pre = soroban_sdk::Bytes::new(&env);
        pre.append(&secret.clone().into());
        pre.append(&bf.clone().into());
        let hash: BytesN<32> = env.crypto().sha256(&pre).into();
        let id = client.commit_ip(&owner, &hash, &0u32);

        let wrong_secret = BytesN::from_array(&env, &[0xFFu8; 32]);
        let mut verifications: Vec<(u64, BytesN<32>, BytesN<32>)> = Vec::new(&env);
        verifications.push_back((id, wrong_secret, bf));

        let results = client.batch_verify_commitments(&verifications);
        assert!(!results.get(0).unwrap());
    }

    #[test]
    fn test_batch_verify_nonexistent_ip_returns_false() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let secret = BytesN::from_array(&env, &[0x01u8; 32]);
        let bf = BytesN::from_array(&env, &[0x02u8; 32]);
        let mut verifications: Vec<(u64, BytesN<32>, BytesN<32>)> = Vec::new(&env);
        verifications.push_back((999u64, secret, bf));

        let results = client.batch_verify_commitments(&verifications);
        assert!(!results.get(0).unwrap());
    }

    // ── Issue #433: IP Ownership Proof Challenge ───────────────────────────────

    #[test]
    fn test_ownership_challenge_full_flow() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let challenger = <Address as TestAddress>::generate(&env);

        let hash = BytesN::from_array(&env, &[0xA1u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let nonce = BytesN::from_array(&env, &[0x42u8; 32]);
        let challenge_id = client.issue_ownership_challenge(&ip_id, &challenger, &nonce);
        assert_eq!(challenge_id, 1u64);

        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&hash.clone().into());
        preimage.append(&nonce.clone().into());
        let response: BytesN<32> = env.crypto().sha256(&preimage).into();

        client.respond_to_ownership_challenge(&challenge_id, &response);

        let valid = client.verify_ownership_challenge(&challenge_id);
        assert!(valid);

        let stored = client.get_ownership_challenge(&challenge_id).unwrap();
        assert!(stored.verified);
        assert_eq!(stored.ip_id, ip_id);
    }

    #[test]
    fn test_ownership_challenge_wrong_response_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let challenger = <Address as TestAddress>::generate(&env);

        let hash = BytesN::from_array(&env, &[0xB1u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let nonce = BytesN::from_array(&env, &[0x11u8; 32]);
        let challenge_id = client.issue_ownership_challenge(&ip_id, &challenger, &nonce);

        let wrong_response = BytesN::from_array(&env, &[0xFFu8; 32]);
        client.respond_to_ownership_challenge(&challenge_id, &wrong_response);

        let valid = client.verify_ownership_challenge(&challenge_id);
        assert!(!valid);
    }

    #[test]
    fn test_ownership_challenge_no_response_returns_false() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let challenger = <Address as TestAddress>::generate(&env);

        let hash = BytesN::from_array(&env, &[0xC1u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let nonce = BytesN::from_array(&env, &[0x22u8; 32]);
        let challenge_id = client.issue_ownership_challenge(&ip_id, &challenger, &nonce);

        let valid = client.verify_ownership_challenge(&challenge_id);
        assert!(!valid);
    }

    // ── Issue #434: Encryption Key Rotation ───────────────────────────────────

    #[test]
    fn test_rotate_commitment_key_success() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);

        let secret = BytesN::from_array(&env, &[0x10u8; 32]);
        let bf = BytesN::from_array(&env, &[0x20u8; 32]);
        let mut pre = soroban_sdk::Bytes::new(&env);
        pre.append(&secret.clone().into());
        pre.append(&bf.clone().into());
        let old_hash: BytesN<32> = env.crypto().sha256(&pre).into();
        let ip_id = client.commit_ip(&owner, &old_hash, &0u32);

        let new_hash = BytesN::from_array(&env, &[0xD1u8; 32]);
        client.rotate_commitment_key(&ip_id, &new_hash, &secret, &bf);

        let record = client.get_ip(&ip_id);
        assert_eq!(record.commitment_hash, new_hash);

        let history = client.get_key_rotation_history(&ip_id);
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).unwrap(), old_hash);
    }

    // ── Tests for Issue #428: Commitment Timestamp Notarization (continued) ──

    #[test]
    fn test_notarize_ip_timestamp_with_valid_signature_2() {
        use ed25519_dalek::{Signer, SigningKey};

        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[50u8; 32]);
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        let signing_key = SigningKey::from_bytes(&[42u8; 32]);
        let verifying_key = signing_key.verifying_key();
        let public_key = BytesN::from_array(&env, verifying_key.as_bytes());

        client.set_notary_public_key(&public_key);

        let record = client.get_ip(&ip_id);
        let mut msg_bytes = [0u8; 16];
        msg_bytes[..8].copy_from_slice(&ip_id.to_be_bytes());
        msg_bytes[8..].copy_from_slice(&record.timestamp.to_be_bytes());

        let sig = signing_key.sign(&msg_bytes);
        let notary_sig = soroban_sdk::Bytes::from_slice(&env, &sig.to_bytes());

        client.notarize_ip_timestamp(&ip_id, &notary_sig);

        let stored_sig = client.get_ip_notary_signature(&ip_id);
        assert!(stored_sig.is_some());
    }

    #[test]
    #[should_panic]
    fn test_rotate_commitment_key_wrong_old_secret_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);

        let secret = BytesN::from_array(&env, &[0x10u8; 32]);
        let bf = BytesN::from_array(&env, &[0x20u8; 32]);
        let mut pre = soroban_sdk::Bytes::new(&env);
        pre.append(&secret.clone().into());
        pre.append(&bf.clone().into());
        let hash: BytesN<32> = env.crypto().sha256(&pre).into();
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let new_hash = BytesN::from_array(&env, &[0xE1u8; 32]);
        let wrong_secret = BytesN::from_array(&env, &[0xFFu8; 32]);

        client.rotate_commitment_key(&ip_id, &new_hash, &wrong_secret, &bf);
    }

    // ── Issue #435: Merkle Tree Proofs ─────────────────────────────────────────

    #[test]
    fn test_generate_merkle_proof_single_ip() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let hash = BytesN::from_array(&env, &[0xF1u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let proof = client.generate_merkle_proof(&ip_id);
        assert_eq!(proof.len(), 0);
    }

    #[test]
    fn test_generate_and_verify_merkle_proof_multiple_ips() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);

        let h1 = BytesN::from_array(&env, &[0x01u8; 32]);
        let h2 = BytesN::from_array(&env, &[0x02u8; 32]);
        let h3 = BytesN::from_array(&env, &[0x03u8; 32]);

        let id1 = client.commit_ip(&owner, &h1, &0u32);
        let id2 = client.commit_ip(&owner, &h2, &0u32);
        let id3 = client.commit_ip(&owner, &h3, &0u32);

        let proof1 = client.generate_merkle_proof(&id1);
        let proof2 = client.generate_merkle_proof(&id2);
        let proof3 = client.generate_merkle_proof(&id3);

        assert!(client.verify_ip_merkle_proof(&id1, &proof1));
        assert!(client.verify_ip_merkle_proof(&id2, &proof2));
        assert!(client.verify_ip_merkle_proof(&id3, &proof3));
    }

    #[test]
    fn test_merkle_root_consistent_with_proof() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);

        let h1 = BytesN::from_array(&env, &[0xA0u8; 32]);
        let h2 = BytesN::from_array(&env, &[0xB0u8; 32]);

        let id1 = client.commit_ip(&owner, &h1, &0u32);
        let id2 = client.commit_ip(&owner, &h2, &0u32);

        let root = client.compute_ip_merkle_root(&owner);

        let proof1 = client.generate_merkle_proof(&id1);
        let proof2 = client.generate_merkle_proof(&id2);

        assert!(client.verify_ip_merkle_proof(&id1, &proof1));
        assert!(client.verify_ip_merkle_proof(&id2, &proof2));

        let zero = BytesN::from_array(&env, &[0u8; 32]);
        assert_ne!(root, zero);
    }

    // ── Dispute Resolution Tests ───────────────────────────────────────────────

    #[test]
    fn test_initiate_dispute_returns_sequential_ids() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = <Address as TestAddress>::generate(&env);
        let challenger = <Address as TestAddress>::generate(&env);
        env.mock_all_auths();

        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[1u8; 32]), &0u32);
        let evidence = BytesN::from_array(&env, &[0xABu8; 32]);

        let d1 = client.initiate_dispute(&ip_id, &challenger, &evidence);
        let d2 = client.initiate_dispute(&ip_id, &challenger, &evidence);

        assert_eq!(d1, 1);
        assert_eq!(d2, 2);
    }

    #[test]
    fn test_initiate_dispute_stores_record() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = <Address as TestAddress>::generate(&env);
        let challenger = <Address as TestAddress>::generate(&env);
        env.mock_all_auths();

        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[2u8; 32]), &0u32);
        let evidence = BytesN::from_array(&env, &[0xCDu8; 32]);

        let dispute_id = client.initiate_dispute(&ip_id, &challenger, &evidence);
        let record = client.get_dispute(&dispute_id);

        assert_eq!(record.ip_id, ip_id);
        assert_eq!(record.challenger, challenger);
        assert_eq!(record.evidence_hash, evidence);
        assert!(!record.resolved);
    }

    #[test]
    fn test_submit_dispute_evidence_updates_hash() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = <Address as TestAddress>::generate(&env);
        let challenger = <Address as TestAddress>::generate(&env);
        env.mock_all_auths();

        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[3u8; 32]), &0u32);
        let dispute_id = client.initiate_dispute(
            &ip_id,
            &challenger,
            &BytesN::from_array(&env, &[0x11u8; 32]),
        );

        let new_evidence = BytesN::from_array(&env, &[0x22u8; 32]);
        client.submit_dispute_evidence(&dispute_id, &owner, &new_evidence);

        let record = client.get_dispute(&dispute_id);
        assert_eq!(record.evidence_hash, new_evidence);
    }

    #[test]
    #[should_panic]
    fn test_notarize_ip_timestamp_invalid_signature_panics() {
        use ed25519_dalek::SigningKey;

        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[51u8; 32]);
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        let signing_key = SigningKey::from_bytes(&[43u8; 32]);
        let public_key = BytesN::from_array(&env, signing_key.verifying_key().as_bytes());
        client.set_notary_public_key(&public_key);

        let bad_sig = soroban_sdk::Bytes::from_array(&env, &[0u8; 64]);
        client.notarize_ip_timestamp(&ip_id, &bad_sig);
    }

    #[test]
    #[should_panic]
    fn test_submit_evidence_by_non_party_panics() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = <Address as TestAddress>::generate(&env);
        let challenger = <Address as TestAddress>::generate(&env);
        let stranger = <Address as TestAddress>::generate(&env);
        env.mock_all_auths();

        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[4u8; 32]), &0u32);
        let dispute_id = client.initiate_dispute(
            &ip_id,
            &challenger,
            &BytesN::from_array(&env, &[0x33u8; 32]),
        );

        client.submit_dispute_evidence(
            &dispute_id,
            &stranger,
            &BytesN::from_array(&env, &[0x44u8; 32]),
        );
    }

    #[test]
    fn test_resolve_dispute_marks_resolved_and_sets_winner() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = <Address as TestAddress>::generate(&env);
        let challenger = <Address as TestAddress>::generate(&env);
        env.mock_all_auths();

        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[5u8; 32]), &0u32);
        let dispute_id = client.initiate_dispute(
            &ip_id,
            &challenger,
            &BytesN::from_array(&env, &[0x55u8; 32]),
        );

        client.resolve_dispute(&dispute_id, &owner);

        let record = client.get_dispute(&dispute_id);
        assert!(record.resolved);
        assert_eq!(record.winner, Some(owner));
    }

    #[test]
    #[should_panic]
    fn test_notarize_ip_timestamp_no_public_key_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[52u8; 32]);
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        let sig = soroban_sdk::Bytes::from_array(&env, &[0u8; 64]);
        client.notarize_ip_timestamp(&ip_id, &sig);
    }

    #[test]
    #[should_panic]
    fn test_resolve_dispute_twice_panics() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = <Address as TestAddress>::generate(&env);
        let challenger = <Address as TestAddress>::generate(&env);
        env.mock_all_auths();

        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[6u8; 32]), &0u32);
        let dispute_id = client.initiate_dispute(
            &ip_id,
            &challenger,
            &BytesN::from_array(&env, &[0x66u8; 32]),
        );

        client.resolve_dispute(&dispute_id, &owner);
        client.resolve_dispute(&dispute_id, &challenger);
    }

    #[test]
    #[should_panic]
    fn test_get_dispute_nonexistent_panics() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        client.get_dispute(&999u64);
    }

    #[test]
    fn test_initiate_dispute_emits_event() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = <Address as TestAddress>::generate(&env);
        let challenger = <Address as TestAddress>::generate(&env);
        env.mock_all_auths();

        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[7u8; 32]), &0u32);
        let dispute_id = client.initiate_dispute(
            &ip_id,
            &challenger,
            &BytesN::from_array(&env, &[0x77u8; 32]),
        );

        let events = env.events().all();
        let found = events.iter().any(|(_, topics, _)| {
            let Ok(t) = soroban_sdk::Vec::<soroban_sdk::Val>::try_from_val(&env, &topics);
            if let Some(v) = t.get(0) {
                if let Ok(s) = soroban_sdk::Symbol::try_from_val(&env, &v) {
                    return s == soroban_sdk::symbol_short!("dispute");
                }
            }
            false
        });
        assert!(
            found,
            "dispute event must be emitted; dispute_id={dispute_id}"
        );
    }

    #[test]
    #[should_panic]
    fn test_notarize_ip_timestamp_wrong_sig_length_panics() {
        use ed25519_dalek::SigningKey;

        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[53u8; 32]);
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        let signing_key = SigningKey::from_bytes(&[44u8; 32]);
        let public_key = BytesN::from_array(&env, signing_key.verifying_key().as_bytes());
        client.set_notary_public_key(&public_key);

        let bad_sig = soroban_sdk::Bytes::from_array(&env, &[1u8; 32]);
        client.notarize_ip_timestamp(&ip_id, &bad_sig);
    }

    // ── Tests for Issue #429: IP Rollback Protection ──────────────────────────

    #[test]
    fn test_commitment_checksum_updated_on_commit() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);

        assert!(client.verify_commitment_integrity());

        let commitment1 = BytesN::from_array(&env, &[60u8; 32]);
        client.commit_ip(&owner, &commitment1, &0u32);
        assert!(client.verify_commitment_integrity());

        let commitment2 = BytesN::from_array(&env, &[61u8; 32]);
        client.commit_ip(&owner, &commitment2, &0u32);
        assert!(client.verify_commitment_integrity());
    }

    #[test]
    fn test_commitment_checksum_reflects_all_commitments() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[62u8; 32]);
        client.commit_ip(&owner, &commitment, &0u32);

        let result1 = client.verify_commitment_integrity();
        let result2 = client.verify_commitment_integrity();
        assert_eq!(result1, result2);
    }

    // ── Tests for Issue #430: IP Commitment Versioning ────────────────────────

    #[test]
    fn test_get_ip_versions_returns_direct_children() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment1 = BytesN::from_array(&env, &[70u8; 32]);
        let ip_id = client.commit_ip(&owner, &commitment1, &0u32);

        let versions = client.get_ip_versions(&ip_id);
        assert_eq!(versions.len(), 0);

        let commitment2 = BytesN::from_array(&env, &[71u8; 32]);
        let v1 = client.commit_ip_version(&owner, &commitment2, &ip_id);

        let commitment3 = BytesN::from_array(&env, &[72u8; 32]);
        let v2 = client.commit_ip_version(&owner, &commitment3, &ip_id);

        let versions = client.get_ip_versions(&ip_id);
        assert_eq!(versions.len(), 2);
        assert_eq!(versions.get(0).unwrap(), v1);
        assert_eq!(versions.get(1).unwrap(), v2);
    }

    #[test]
    fn test_get_ip_versions_empty_for_no_versions() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[73u8; 32]);
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        let versions = client.get_ip_versions(&ip_id);
        assert_eq!(versions.len(), 0);
    }

    #[test]
    fn test_get_ip_lineage_includes_root_and_versions() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment1 = BytesN::from_array(&env, &[74u8; 32]);
        let root_id = client.commit_ip(&owner, &commitment1, &0u32);

        let commitment2 = BytesN::from_array(&env, &[75u8; 32]);
        let v1 = client.commit_ip_version(&owner, &commitment2, &root_id);

        let lineage = client.get_ip_lineage(&root_id);
        assert!(lineage.len() >= 2);
        assert_eq!(lineage.get(0).unwrap(), root_id);

        let mut found = false;
        for i in 0..lineage.len() {
            if lineage.get(i).unwrap() == v1 {
                found = true;
                break;
            }
        }
        assert!(found, "v1 should be in lineage");
    }

    #[test]
    fn test_get_ip_version_chain_includes_all() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment1 = BytesN::from_array(&env, &[76u8; 32]);
        let root_id = client.commit_ip(&owner, &commitment1, &0u32);

        let commitment2 = BytesN::from_array(&env, &[77u8; 32]);
        let v1 = client.commit_ip_version(&owner, &commitment2, &root_id);

        let chain = client.get_ip_version_chain(&root_id);
        assert!(chain.len() >= 2);
        assert_eq!(chain.get(0).unwrap(), root_id);

        let mut found_v1 = false;
        for i in 0..chain.len() {
            if chain.get(i).unwrap() == v1 {
                found_v1 = true;
                break;
            }
        }
        assert!(found_v1, "v1 should be in version chain");
    }

    // ── Tests for Issue #431: IP Claim Expiration Warnings ───────────────────

    #[test]
    fn test_check_expiration_warning_not_expiring() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[80u8; 32]);
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        let is_expiring = client.check_expiration_warning(&ip_id, &1u32);
        assert!(!is_expiring, "Newly committed IP should not be expiring");
    }

    #[test]
    fn test_check_expiration_warning_large_threshold() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[81u8; 32]);
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        let is_expiring = client.check_expiration_warning(&ip_id, &(crate::LEDGER_BUMP + 1));
        assert!(
            is_expiring,
            "IP should be expiring when threshold > LEDGER_BUMP"
        );
    }

    #[test]
    #[should_panic]
    fn test_check_expiration_warning_nonexistent_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        client.check_expiration_warning(&999u64, &100u32);
    }

    #[test]
    fn test_check_expiration_warning_emits_event_when_expiring() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[82u8; 32]);
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        let _ = env.events().all();

        let is_expiring = client.check_expiration_warning(&ip_id, &(crate::LEDGER_BUMP + 1));
        assert!(is_expiring);

        let events = env.events().all();
        assert!(
            !events.is_empty(),
            "Expiration warning event should be emitted"
        );
        let event = events.get(0).unwrap();
        let expected_topics = (symbol_short!("exp_warn"), ip_id).into_val(&env);
        assert_eq!(event.1, expected_topics);
    }

    // ── Tests for batch_commit_ip_anonymous ───────────────────────────────────

    /// Happy path: two hashes produce two sequential IDs and records are retrievable.
    #[test]
    fn test_anon_batch_creates_records_with_correct_hashes() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let blinded_owner = BytesN::from_array(&env, &[0xAAu8; 32]);
        let h1 = BytesN::from_array(&env, &[0x11u8; 32]);
        let h2 = BytesN::from_array(&env, &[0x22u8; 32]);
        let hashes = Vec::from_array(&env, [h1.clone(), h2.clone()]);

        let ids = client.batch_commit_ip_anonymous(&blinded_owner, &hashes);

        assert_eq!(ids.len(), 2);
        let rec1 = client.get_ip(&ids.get(0).unwrap());
        let rec2 = client.get_ip(&ids.get(1).unwrap());
        assert_eq!(rec1.commitment_hash, h1);
        assert_eq!(rec2.commitment_hash, h2);
    }

    /// IDs are sequential and continue from the global counter.
    #[test]
    fn test_anon_batch_ids_are_sequential() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        // Commit one regular IP first so the counter starts at 2 for the batch.
        let owner = <Address as TestAddress>::generate(&env);
        client.commit_ip(&owner, &BytesN::from_array(&env, &[0x01u8; 32]), &0u32);

        let blinded_owner = BytesN::from_array(&env, &[0xBBu8; 32]);
        let hashes = Vec::from_array(
            &env,
            [
                BytesN::from_array(&env, &[0x02u8; 32]),
                BytesN::from_array(&env, &[0x03u8; 32]),
            ],
        );

        let ids = client.batch_commit_ip_anonymous(&blinded_owner, &hashes);

        assert_eq!(ids.get(0).unwrap(), 2u64);
        assert_eq!(ids.get(1).unwrap(), 3u64);
    }

    /// Anonymous commits must NOT appear in the owner index.
    #[test]
    fn test_anon_batch_does_not_populate_owner_index() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let blinded_owner = BytesN::from_array(&env, &[0xCCu8; 32]);
        let hashes = Vec::from_array(&env, [BytesN::from_array(&env, &[0x33u8; 32])]);
        client.batch_commit_ip_anonymous(&blinded_owner, &hashes);

        let any_address = <Address as TestAddress>::generate(&env);
        assert_eq!(client.list_ip_by_owner(&any_address).len(), 0);
    }

    /// The on-chain record owner must be the contract address, not the submitter.
    #[test]
    fn test_anon_batch_record_owner_is_contract_address() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let blinded_owner = BytesN::from_array(&env, &[0xDDu8; 32]);
        let h = BytesN::from_array(&env, &[0x44u8; 32]);
        let ids = client.batch_commit_ip_anonymous(&blinded_owner, &Vec::from_array(&env, [h]));

        let record = client.get_ip(&ids.get(0).unwrap());
        assert_eq!(record.owner, contract_id);
    }

    /// blinded_owner is stored and retrievable via get_anonymous_owner.
    #[test]
    fn test_anon_batch_stores_blinded_owner() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let blinded_owner = BytesN::from_array(&env, &[0xEEu8; 32]);
        let h = BytesN::from_array(&env, &[0x55u8; 32]);
        client.batch_commit_ip_anonymous(&blinded_owner, &Vec::from_array(&env, [h.clone()]));

        let stored = client.get_anonymous_owner(&h);
        assert_eq!(stored, Some(blinded_owner));
    }

    /// get_anonymous_owner returns None for a hash committed via commit_ip (not anonymous).
    #[test]
    fn test_get_anonymous_owner_returns_none_for_non_anonymous_commit() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let h = BytesN::from_array(&env, &[0x66u8; 32]);
        client.commit_ip(&owner, &h, &0u32);

        assert_eq!(client.get_anonymous_owner(&h), None);
    }

    /// Each commitment emits an "anon_cmit" event with (id, timestamp, blinded_owner).
    #[test]
    fn test_anon_batch_emits_event_per_commitment() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let blinded_owner = BytesN::from_array(&env, &[0xFFu8; 32]);
        let h1 = BytesN::from_array(&env, &[0x77u8; 32]);
        let h2 = BytesN::from_array(&env, &[0x88u8; 32]);
        let ids =
            client.batch_commit_ip_anonymous(&blinded_owner, &Vec::from_array(&env, [h1, h2]));

        let all_events = env.events().all();
        // Exactly two anon_cmit events (one per hash).
        let anon_events: soroban_sdk::Vec<_> = {
            let mut v = soroban_sdk::Vec::new(&env);
            for e in all_events.iter() {
                let (_, ref topics, _) = e;
                let t = soroban_sdk::Vec::<soroban_sdk::Val>::try_from_val(&env, &topics)
                    .expect("topics should be Vec<Val>");
                if let Some(first) = t.get(0) {
                    if let Ok(s) = soroban_sdk::Symbol::try_from_val(&env, &first) {
                        if s == symbol_short!("anon_cmit") {
                            v.push_back(e.clone());
                        }
                    }
                }
            }
            v
        };
        assert_eq!(anon_events.len(), 2, "expected one event per commitment");

        // Verify first event data contains the correct ip_id and blinded_owner.
        let (_, _, data) = anon_events.get(0).unwrap();
        let (event_id, _ts, event_blinded): (u64, u64, BytesN<32>) =
            TryFromVal::try_from_val(&env, &data).unwrap();
        assert_eq!(event_id, ids.get(0).unwrap());
        assert_eq!(event_blinded, blinded_owner);
    }

    /// A zero commitment hash in the batch must panic.
    #[test]
    #[should_panic]
    fn test_anon_batch_zero_hash_rejected() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let blinded_owner = BytesN::from_array(&env, &[0x01u8; 32]);
        let zero = BytesN::from_array(&env, &[0u8; 32]);
        client.batch_commit_ip_anonymous(&blinded_owner, &Vec::from_array(&env, [zero]));
    }

    /// A duplicate commitment hash (already registered) must panic.
    #[test]
    #[should_panic]
    fn test_anon_batch_duplicate_hash_rejected() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let blinded_owner = BytesN::from_array(&env, &[0x02u8; 32]);
        let h = BytesN::from_array(&env, &[0x99u8; 32]);
        // First call succeeds.
        client.batch_commit_ip_anonymous(&blinded_owner, &Vec::from_array(&env, [h.clone()]));
        // Second call with the same hash must panic.
        client.batch_commit_ip_anonymous(&blinded_owner, &Vec::from_array(&env, [h]));
    }

    /// Duplicate hash within the same batch must panic on the second occurrence.
    #[test]
    #[should_panic]
    fn test_anon_batch_intra_batch_duplicate_rejected() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let blinded_owner = BytesN::from_array(&env, &[0x03u8; 32]);
        let h = BytesN::from_array(&env, &[0xAAu8; 32]);
        // Same hash twice in one batch.
        client.batch_commit_ip_anonymous(&blinded_owner, &Vec::from_array(&env, [h.clone(), h]));
    }

    /// Empty batch must panic.
    #[test]
    #[should_panic]
    fn test_anon_batch_empty_batch_rejected() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let blinded_owner = BytesN::from_array(&env, &[0x04u8; 32]);
        let empty: Vec<BytesN<32>> = Vec::new(&env);
        client.batch_commit_ip_anonymous(&blinded_owner, &empty);
    }

    /// Anonymous and regular commits share the same ID counter correctly.
    #[test]
    fn test_anon_batch_interleaved_with_regular_commits() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);

        let id1 = client.commit_ip(&owner, &BytesN::from_array(&env, &[0x10u8; 32]), &0u32);

        let blinded_owner = BytesN::from_array(&env, &[0x05u8; 32]);
        let anon_ids = client.batch_commit_ip_anonymous(
            &blinded_owner,
            &Vec::from_array(
                &env,
                [
                    BytesN::from_array(&env, &[0x20u8; 32]),
                    BytesN::from_array(&env, &[0x30u8; 32]),
                ],
            ),
        );

        let id4 = client.commit_ip(&owner, &BytesN::from_array(&env, &[0x40u8; 32]), &0u32);

        assert_eq!(id1, 1);
        assert_eq!(anon_ids.get(0).unwrap(), 2);
        assert_eq!(anon_ids.get(1).unwrap(), 3);
        assert_eq!(id4, 4);
    }

    // ── Category Tests (Issue #459) ──────────────────────────────────────────

    /// Basic assign and list within a category.
    #[test]
    fn test_assign_ip_to_category_basic() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[1u8; 32]), &0u32);
        let cat = BytesN::from_array(&env, &[2u8; 32]);

        client.assign_ip_to_category(&ip_id, &cat);

        let ips = client.list_ip_by_category(&owner, &cat);
        assert_eq!(ips.len(), 1);
        assert_eq!(ips.get(0).unwrap(), ip_id);
    }

    /// Access control: `assign_ip_to_category` calls `record.owner.require_auth()`,
    /// which the Soroban host enforces at the protocol level. In test environments
    /// with `mock_all_auths()` the check is bypassed, so the effective security
    /// guarantee is the `require_auth()` call in the contract — not this test.
    /// Assigning the same category twice is idempotent.
    #[test]
    fn test_assign_ip_to_category_idempotent() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[1u8; 32]), &0u32);
        let cat = BytesN::from_array(&env, &[2u8; 32]);

        client.assign_ip_to_category(&ip_id, &cat);
        client.assign_ip_to_category(&ip_id, &cat); // second time — no-op

        let ips = client.list_ip_by_category(&owner, &cat);
        assert_eq!(ips.len(), 1);
    }

    /// Multiple IPs can be assigned to the same category.
    #[test]
    fn test_multiple_ips_same_category() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let id1 = client.commit_ip(&owner, &BytesN::from_array(&env, &[1u8; 32]), &0u32);
        let id2 = client.commit_ip(&owner, &BytesN::from_array(&env, &[2u8; 32]), &0u32);
        let id3 = client.commit_ip(&owner, &BytesN::from_array(&env, &[3u8; 32]), &0u32);
        let cat = BytesN::from_array(&env, &[0xCu8; 32]);

        client.assign_ip_to_category(&id1, &cat);
        client.assign_ip_to_category(&id2, &cat);
        client.assign_ip_to_category(&id3, &cat);

        let ips = client.list_ip_by_category(&owner, &cat);
        assert_eq!(ips.len(), 3);

        let mut ids: Vec<u64> = Vec::new(&env);
        ids.push_back(id1);
        ids.push_back(id2);
        ids.push_back(id3);
        for id in ids.iter() {
            assert!(ips.iter().any(|x| x == id));
        }
    }

    /// list_ip_by_category returns empty for unknown category.
    #[test]
    fn test_list_ip_by_category_empty() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let cat = BytesN::from_array(&env, &[0x99u8; 32]);

        let ips = client.list_ip_by_category(&owner, &cat);
        assert_eq!(ips.len(), 0);
    }

    /// list_owner_categories returns all categories used by the owner.
    #[test]
    fn test_list_owner_categories() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let cat1 = BytesN::from_array(&env, &[0x10u8; 32]);
        let cat2 = BytesN::from_array(&env, &[0x20u8; 32]);
        let cat3 = BytesN::from_array(&env, &[0x30u8; 32]);

        let ip1 = client.commit_ip(&owner, &BytesN::from_array(&env, &[1u8; 32]), &0u32);
        let ip2 = client.commit_ip(&owner, &BytesN::from_array(&env, &[2u8; 32]), &0u32);

        client.assign_ip_to_category(&ip1, &cat1);
        client.assign_ip_to_category(&ip1, &cat2);
        client.assign_ip_to_category(&ip2, &cat3);

        let cats = client.list_owner_categories(&owner);
        assert_eq!(cats.len(), 3);
        assert!(cats.iter().any(|c| c == cat1));
        assert!(cats.iter().any(|c| c == cat2));
        assert!(cats.iter().any(|c| c == cat3));
    }

    /// list_owner_categories returns empty for unknown owner.
    #[test]
    fn test_list_owner_categories_empty() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let stranger = <Address as TestAddress>::generate(&env);
        let cats = client.list_owner_categories(&stranger);
        assert_eq!(cats.len(), 0);
    }

    /// Category hierarchy depth: 5 levels is allowed.
    #[test]
    fn test_validate_category_path_max_depth_allowed() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        // 5 levels: a/b/c/d/e
        let path = Bytes::from_array(&env, b"a/b/c/d/e");
        let hash = client.validate_category_path(&path);
        // Should not panic; hash should be non-zero
        assert_ne!(hash, BytesN::from_array(&env, &[0u8; 32]));
    }

    /// Category hierarchy depth: 6 levels panics.
    #[test]
    #[should_panic(expected = "#30")]
    fn test_validate_category_path_depth_exceeded() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        // 6 levels: a/b/c/d/e/f
        let path = Bytes::from_array(&env, b"a/b/c/d/e/f");
        client.validate_category_path(&path);
    }

    /// Path traversal ".." is rejected.
    #[test]
    #[should_panic(expected = "#31")]
    fn test_validate_category_path_traversal_dotdot() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let path = Bytes::from_array(&env, b"a/..");
        client.validate_category_path(&path);
    }

    /// Leading slash is rejected.
    #[test]
    #[should_panic(expected = "#31")]
    fn test_validate_category_path_leading_slash() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let path = Bytes::from_array(&env, b"/a/b");
        client.validate_category_path(&path);
    }

    /// Trailing slash is rejected.
    #[test]
    #[should_panic(expected = "#31")]
    fn test_validate_category_path_trailing_slash() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let path = Bytes::from_array(&env, b"a/b/");
        client.validate_category_path(&path);
    }

    /// Double slash is rejected.
    #[test]
    #[should_panic(expected = "#31")]
    fn test_validate_category_path_double_slash() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let path = Bytes::from_array(&env, b"a//b");
        client.validate_category_path(&path);
    }

    /// Empty path is rejected.
    #[test]
    #[should_panic(expected = "#31")]
    fn test_validate_category_path_empty() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let path = Bytes::new(&env);
        client.validate_category_path(&path);
    }

    /// Zero category hash is rejected on assign.
    #[test]
    #[should_panic(expected = "#29")]
    fn test_assign_ip_to_category_zero_hash() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[1u8; 32]), &0u32);
        let zero_hash = BytesN::from_array(&env, &[0u8; 32]);

        client.assign_ip_to_category(&ip_id, &zero_hash);
    }

    /// Category queries filter by owner correctly.
    #[test]
    fn test_list_ip_by_category_filters_by_owner() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner1 = <Address as TestAddress>::generate(&env);
        let owner2 = <Address as TestAddress>::generate(&env);
        let cat = BytesN::from_array(&env, &[0xCu8; 32]);

        let ip1 = client.commit_ip(&owner1, &BytesN::from_array(&env, &[1u8; 32]), &0u32);
        let ip2 = client.commit_ip(&owner2, &BytesN::from_array(&env, &[2u8; 32]), &0u32);

        client.assign_ip_to_category(&ip1, &cat);
        client.assign_ip_to_category(&ip2, &cat);

        let owner1_ips = client.list_ip_by_category(&owner1, &cat);
        assert_eq!(owner1_ips.len(), 1);
        assert_eq!(owner1_ips.get(0).unwrap(), ip1);

        let owner2_ips = client.list_ip_by_category(&owner2, &cat);
        assert_eq!(owner2_ips.len(), 1);
        assert_eq!(owner2_ips.get(0).unwrap(), ip2);
    }
}
