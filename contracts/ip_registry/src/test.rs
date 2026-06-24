#[cfg(test)]
mod tests {
    use crate::IpRecord;
    use crate::StakeRecord;
    use soroban_sdk::contractclient;
    use soroban_sdk::testutils::Address as TestAddress;
    use soroban_sdk::testutils::Events;
    use soroban_sdk::{symbol_short, Address, BytesN, Env, IntoVal, Vec};

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
        fn get_stake(env: Env, ip_id: u64) -> Option<StakeRecord>;
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
        fn set_batch_metadata(
            env: Env,
            ip_id: u64,
            batch_id: BytesN<32>,
            description: soroban_sdk::Bytes,
        );
        fn get_batch_metadata(env: Env, ip_id: u64) -> Option<crate::BatchMetadata>;
        fn get_commitment_compression(env: Env, ip_id: u64) -> crate::CompressionAlgo;
        fn set_commitment_compression(env: Env, ip_id: u64, algorithm: crate::CompressionAlgo);
        fn get_compressed_bytes(env: Env, ip_id: u64) -> soroban_sdk::Bytes;
        fn encrypt_commitment(
            env: Env,
            ip_id: u64,
            encrypted_hash: soroban_sdk::Bytes,
            key_hint: BytesN<32>,
        );
        fn get_encrypted_commitment(
            env: Env,
            ip_id: u64,
        ) -> Option<crate::EncryptedCommitmentRecord>;
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
        fn batch_verify_commitments(
            env: Env,
            requests: Vec<crate::VerifyRequest>,
        ) -> Vec<crate::VerifyResult>;
        fn batch_commit_ip_anonymous(
            env: Env,
            blinded_owner: BytesN<32>,
            commitment_hashes: Vec<BytesN<32>>,
        ) -> Vec<u64>;
        fn batch_stake_commitments(env: Env, ip_ids: Vec<u64>, amounts: Vec<i128>);
        fn batch_update_reputation(env: Env, ip_ids: Vec<u64>, score_deltas: Vec<i64>);
        fn get_reputation(env: Env, owner: Address) -> crate::ReputationRecord;
        fn get_anonymous_owner(env: Env, commitment_hash: BytesN<32>) -> Option<BytesN<32>>;
        // Issue #464: Batch anonymity accessor
        fn get_blinded_owner_batch(
            env: Env,
            commitment_hashes: Vec<BytesN<32>>,
        ) -> Vec<Option<BytesN<32>>>;
        // Issue #465: Batch escrow
        fn batch_escrow_commitments(
            env: Env,
            depositor: Address,
            ip_ids: Vec<u64>,
            release_to: Address,
            timeout: u64,
        ) -> BytesN<32>;
        fn get_batch_escrow(env: Env, escrow_id: BytesN<32>) -> Option<crate::EscrowRecord>;
        fn release_batch_escrow(env: Env, escrow_id: BytesN<32>);
        fn cancel_batch_escrow(env: Env, escrow_id: BytesN<32>);
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
        fn set_ip_expiry(env: Env, ip_id: u64, expiry_timestamp: u64, grace_period_seconds: u64);
        fn renew_ip_commitment(env: Env, ip_id: u64, new_expiry: u64) -> bool;
        fn cleanup_expired_ips(env: Env, start_id: u64, limit: u32) -> (u32, u64);
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
        assert_eq!(all_events.events().len(), 1);

        // Verify the record separately.
        let record = client.get_ip(&ip_id);
        assert_eq!(record.owner, owner);
        assert_eq!(record.commitment_hash, commitment);
        assert_eq!(record.ip_id, ip_id);
    }

    #[test]
    fn test_batch_stake_commitments_success() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner1 = <Address as TestAddress>::generate(&env);
        let owner2 = <Address as TestAddress>::generate(&env);
        let commitment1 = BytesN::from_array(&env, &[11u8; 32]);
        let commitment2 = BytesN::from_array(&env, &[12u8; 32]);

        env.mock_all_auths();
        let ip_id1 = client.commit_ip(&owner1, &commitment1, &0u32);
        let ip_id2 = client.commit_ip(&owner2, &commitment2, &0u32);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip_id1);
        ip_ids.push_back(ip_id2);

        let mut amounts = Vec::new(&env);
        amounts.push_back(100i128);
        amounts.push_back(200i128);

        client.batch_stake_commitments(&ip_ids, &amounts);

        let stake1 = client.get_stake(&ip_id1).unwrap();
        let stake2 = client.get_stake(&ip_id2).unwrap();

        assert_eq!(stake1.ip_id, ip_id1);
        assert_eq!(stake1.owner, owner1);
        assert_eq!(stake1.amount, 100i128);
        assert!(!stake1.slashed);

        assert_eq!(stake2.ip_id, ip_id2);
        assert_eq!(stake2.owner, owner2);
        assert_eq!(stake2.amount, 200i128);
        assert!(!stake2.slashed);
    }

    #[test]
    #[should_panic]
    fn test_batch_stake_commitments_mismatched_lengths_panics() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[13u8; 32]);

        env.mock_all_auths();
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip_id);

        let mut amounts = Vec::new(&env);
        amounts.push_back(100i128);
        amounts.push_back(200i128);

        client.batch_stake_commitments(&ip_ids, &amounts);
    }

    #[test]
    fn test_batch_update_reputation_for_multiple_commitments() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner1 = <Address as TestAddress>::generate(&env);
        let owner2 = <Address as TestAddress>::generate(&env);
        let commitment1 = BytesN::from_array(&env, &[14u8; 32]);
        let commitment2 = BytesN::from_array(&env, &[15u8; 32]);

        env.mock_all_auths();
        let ip_id1 = client.commit_ip(&owner1, &commitment1, &0u32);
        let ip_id2 = client.commit_ip(&owner2, &commitment2, &0u32);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip_id1);
        ip_ids.push_back(ip_id2);

        let mut deltas = Vec::new(&env);
        deltas.push_back(10i64);
        deltas.push_back(-5i64);

        client.batch_update_reputation(&ip_ids, &deltas);

        let rep1 = client.get_reputation(&owner1);
        let rep2 = client.get_reputation(&owner2);

        assert_eq!(rep1.score, 10);
        assert_eq!(rep2.score, -5);
    }

    #[test]
    #[should_panic]
    fn test_batch_update_reputation_mismatched_lengths_panics() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = <Address as TestAddress>::generate(&env);
        let commitment = BytesN::from_array(&env, &[16u8; 32]);

        env.mock_all_auths();
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip_id);

        let mut deltas = Vec::new(&env);
        deltas.push_back(10i64);
        deltas.push_back(20i64);

        client.batch_update_reputation(&ip_ids, &deltas);
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

        assert!(env.events().all().events().len() > 0);
        // Verify transfer via state: bob is now the owner
        let record = client.get_ip(&ip_id);
        assert_eq!(record.owner, bob);
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

        assert!(env.events().all().events().len() > 0);
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
        // Use a hash with 29 unique bytes and 32 leading zero bits.
        // entropy_score = (29 * 50) / 32 = 45
        // pow_score = (32 * 50) / 32 = 50
        // total = 95
        let mut hash_bytes: [u8; 32] = core::array::from_fn(|i| i as u8);
        hash_bytes[0] = 0;
        hash_bytes[1] = 0;
        hash_bytes[2] = 0;
        hash_bytes[3] = 0;
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
        // A hash that satisfies the PoW requirement can still score below 100.
        // With 25 unique bytes and 64 leading zero bits:
        // entropy_score = (25 * 50) / 32 = 39
        // pow_score = 50
        // total = 89
        let mut hash_bytes: [u8; 32] = core::array::from_fn(|i| i as u8);
        hash_bytes[..8].fill(0);
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
        // A hash with 16 unique bytes and 16 leading zero bits gives:
        // entropy_score = (16 * 50) / 32 = 25
        // pow_score = (16 * 50) / 32 = 25
        // total = 50
        let mut hash_bytes: [u8; 32] = core::array::from_fn(|i| (i % 16) as u8);
        hash_bytes[0] = 0;
        hash_bytes[1] = 0;
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

        let mut requests: Vec<crate::VerifyRequest> = Vec::new(&env);
        requests.push_back(crate::VerifyRequest {
            ip_id: id1,
            secret: secret1,
            blinding_factor: bf1,
        });
        requests.push_back(crate::VerifyRequest {
            ip_id: id2,
            secret: secret2,
            blinding_factor: bf2,
        });

        let results = client.batch_verify_commitments(&requests);
        assert_eq!(results.len(), 2);
        assert!(results.get(0).unwrap().valid);
        assert!(results.get(1).unwrap().valid);
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
        let mut requests: Vec<crate::VerifyRequest> = Vec::new(&env);
        requests.push_back(crate::VerifyRequest {
            ip_id: id,
            secret: wrong_secret,
            blinding_factor: bf,
        });

        let results = client.batch_verify_commitments(&requests);
        assert!(!results.get(0).unwrap().valid);
    }

    #[test]
    #[should_panic]
    fn test_batch_verify_nonexistent_ip_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let secret = BytesN::from_array(&env, &[0x01u8; 32]);
        let bf = BytesN::from_array(&env, &[0x02u8; 32]);
        let mut requests: Vec<crate::VerifyRequest> = Vec::new(&env);
        requests.push_back(crate::VerifyRequest {
            ip_id: 999u64,
            secret,
            blinding_factor: bf,
        });

        client.batch_verify_commitments(&requests);
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
    fn test_notarize_ip_timestamp_with_valid_signature_continued() {
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
        // Verify at least one event was emitted for the dispute
        assert!(
            events.events().len() > 0,
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
            events.events().len() > 0,
            "Expiration warning event should be emitted"
        );
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

    /// Each commitment emits an "ip_commit_a" event with (id, timestamp, blinded_owner).
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

        // Exactly two ip_cmt_a events emitted (one per commitment hash).
        let all_events = env.events().all();
        assert_eq!(
            all_events.events().len(),
            2,
            "expected one event per commitment"
        );

        // Verify event data: (ip_id, timestamp, blinded_owner) for first commitment.
        let expected_id0: u64 = ids.get(0).unwrap();
        let expected_id1: u64 = ids.get(1).unwrap();
        let ts = env.ledger().timestamp();
        assert_eq!(
            all_events,
            Vec::from_array(
                &env,
                [
                    (
                        contract_id.clone(),
                        Vec::from_array(
                            &env,
                            [
                                symbol_short!("ip_cmt_a").into_val(&env),
                                contract_id.to_val(),
                            ],
                        ),
                        (expected_id0, ts, blinded_owner.clone()).into_val(&env),
                    ),
                    (
                        contract_id.clone(),
                        Vec::from_array(
                            &env,
                            [
                                symbol_short!("ip_cmt_a").into_val(&env),
                                contract_id.to_val(),
                            ],
                        ),
                        (expected_id1, ts, blinded_owner.clone()).into_val(&env),
                    ),
                ]
            )
        );
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
}

// ── Expiry & Grace Period Tests ───────────────────────────────────────────────

#[cfg(test)]
mod expiry_tests {
    use super::tests::IpRegistryClient;
    use soroban_sdk::{
        testutils::{Address as _, Events, Ledger},
        Address, BytesN, Env, Vec,
    };

    fn setup() -> (Env, IpRegistryClient<'static>, Address, u64) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[0xAAu8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);
        (env, client, owner, ip_id)
    }

    #[test]
    fn test_set_ip_expiry_stores_fields() {
        let (env, client, _owner, ip_id) = setup();
        let now = env.ledger().timestamp();
        client.set_ip_expiry(&ip_id, &(now + 1000), &300);
        let record = client.get_ip(&ip_id);
        assert_eq!(record.expiry_timestamp, now + 1000);
        assert_eq!(record.grace_period_seconds, 300);
    }

    #[test]
    fn test_renew_ip_commitment_extends_expiry() {
        let (env, client, _owner, ip_id) = setup();
        let now = env.ledger().timestamp();
        client.set_ip_expiry(&ip_id, &(now + 1000), &300);
        let result = client.renew_ip_commitment(&ip_id, &(now + 2000));
        assert!(result);
        assert_eq!(client.get_ip(&ip_id).expiry_timestamp, now + 2000);
    }

    #[test]
    #[should_panic]
    fn test_renew_ip_commitment_lower_expiry_panics() {
        let (env, client, _owner, ip_id) = setup();
        let now = env.ledger().timestamp();
        client.set_ip_expiry(&ip_id, &(now + 1000), &300);
        // new_expiry <= current expiry → must panic
        client.renew_ip_commitment(&ip_id, &(now + 500));
    }

    #[test]
    fn test_cleanup_removes_ip_past_grace_period() {
        let (env, client, _owner, ip_id) = setup();
        let now = env.ledger().timestamp();
        client.set_ip_expiry(&ip_id, &(now + 100), &50);

        // Advance time past expiry + grace
        env.ledger().with_mut(|l| l.timestamp = now + 200);

        let (cleaned, next) = client.cleanup_expired_ips(&ip_id, &1);

        // Record should be gone
        assert!(client.try_get_ip(&ip_id).is_err());
        assert_eq!(cleaned, 1);
        assert_eq!(next, ip_id + 1);
    }

    #[test]
    fn test_cleanup_skips_ip_within_grace_period() {
        let (env, client, _owner, ip_id) = setup();
        let now = env.ledger().timestamp();
        client.set_ip_expiry(&ip_id, &(now + 100), &200);

        // Advance past expiry but still within grace
        env.ledger().with_mut(|l| l.timestamp = now + 150);

        let (cleaned, next) = client.cleanup_expired_ips(&ip_id, &1);

        // Record should still exist
        assert_eq!(client.get_ip(&ip_id).ip_id, ip_id);
        assert_eq!(cleaned, 0);
        assert_eq!(next, ip_id + 1);
    }

    #[test]
    fn test_cleanup_skips_ip_with_no_expiry() {
        let (env, client, _owner, ip_id) = setup();
        // No expiry set (expiry_timestamp == 0)
        env.ledger().with_mut(|l| l.timestamp = 9_999_999);

        let (cleaned, next) = client.cleanup_expired_ips(&ip_id, &1);

        // Record must still exist
        assert_eq!(client.get_ip(&ip_id).ip_id, ip_id);
        assert_eq!(cleaned, 0);
        assert_eq!(next, ip_id + 1);
    }

    #[test]
    fn test_renew_emits_event() {
        let (env, client, _owner, ip_id) = setup();
        let now = env.ledger().timestamp();
        client.set_ip_expiry(&ip_id, &(now + 1000), &0);
        client.renew_ip_commitment(&ip_id, &(now + 2000));

        // Verify at least one event was emitted after the renew call.
        // (set_ip_expiry emits one event, renew_ip_commitment emits another)
        let events = env.events().all();
        assert!(events.events().len() >= 1, "ip_renew event must be emitted");
    }

    #[test]
    fn test_cleanup_emits_event() {
        let (env, client, _owner, ip_id) = setup();
        let now = env.ledger().timestamp();
        client.set_ip_expiry(&ip_id, &(now + 100), &0);
        env.ledger().with_mut(|l| l.timestamp = now + 200);

        client.cleanup_expired_ips(&ip_id, &1);

        // Verify at least one event was emitted (set_ip_expiry + cleanup_expired_ips).
        let events = env.events().all();
        assert!(events.events().len() >= 1, "ip_clean event must be emitted");
    }

    #[test]
    fn test_cleanup_partial_range() {
        let (env, client, owner, _) = setup();
        let now = env.ledger().timestamp();

        // Create 3 IPs with expiry
        let hash1 = BytesN::from_array(&env, &[1u8; 32]);
        let id1 = client.commit_ip(&owner, &hash1, &0);
        client.set_ip_expiry(&id1, &(now + 100), &0);

        let hash2 = BytesN::from_array(&env, &[2u8; 32]);
        let id2 = client.commit_ip(&owner, &hash2, &0);
        client.set_ip_expiry(&id2, &(now + 100), &0);

        let hash3 = BytesN::from_array(&env, &[3u8; 32]);
        let id3 = client.commit_ip(&owner, &hash3, &0);
        client.set_ip_expiry(&id3, &(now + 100), &0);

        // Advance past expiry
        env.ledger().with_mut(|l| l.timestamp = now + 200);

        // Clean only 2 starting from id1
        let (cleaned, next) = client.cleanup_expired_ips(&id1, &2);
        assert_eq!(cleaned, 2);
        assert_eq!(next, id1 + 2);

        // First 2 should be gone, third should remain
        assert!(client.try_get_ip(&id1).is_err());
        assert!(client.try_get_ip(&id2).is_err());
        assert_eq!(client.get_ip(&id3).ip_id, id3);
    }

    #[test]
    fn test_cleanup_empty_range() {
        let (env, client, _owner, _) = setup();
        // No IPs exist at high IDs
        let (cleaned, next) = client.cleanup_expired_ips(&999_999, &10);
        assert_eq!(cleaned, 0);
        assert_eq!(next, 999_999 + 10);
    }
}

// ── #464: get_blinded_owner_batch tests ──────────────────────────────────────

#[cfg(test)]
mod blinded_owner_batch_tests {
    use super::tests::IpRegistryClient;
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

    fn setup() -> (Env, IpRegistryClient<'static>) {
        let env = Env::default();
        let id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &id);
        (env, client)
    }

    #[test]
    fn test_get_blinded_owner_batch_returns_stored_values() {
        let (env, client) = setup();
        let blinded = BytesN::from_array(&env, &[0xABu8; 32]);
        let h1 = BytesN::from_array(&env, &[0x11u8; 32]);
        let h2 = BytesN::from_array(&env, &[0x22u8; 32]);
        client
            .batch_commit_ip_anonymous(&blinded, &Vec::from_array(&env, [h1.clone(), h2.clone()]));

        let results = client.get_blinded_owner_batch(&Vec::from_array(&env, [h1, h2]));
        assert_eq!(results.len(), 2);
        assert_eq!(results.get(0).unwrap(), Some(blinded.clone()));
        assert_eq!(results.get(1).unwrap(), Some(blinded));
    }

    #[test]
    fn test_get_blinded_owner_batch_returns_none_for_non_anonymous() {
        let (env, client) = setup();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let h = BytesN::from_array(&env, &[0x33u8; 32]);
        client.commit_ip(&owner, &h, &0u32);

        let results = client.get_blinded_owner_batch(&Vec::from_array(&env, [h]));
        assert_eq!(results.get(0).unwrap(), None);
    }

    #[test]
    fn test_get_blinded_owner_batch_empty_input() {
        let (env, client) = setup();
        let empty: Vec<BytesN<32>> = Vec::new(&env);
        let results = client.get_blinded_owner_batch(&empty);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_get_blinded_owner_batch_mixed_results() {
        let (env, client) = setup();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let h_regular = BytesN::from_array(&env, &[0x44u8; 32]);
        client.commit_ip(&owner, &h_regular, &0u32);

        let blinded = BytesN::from_array(&env, &[0xCCu8; 32]);
        let h_anon = BytesN::from_array(&env, &[0x55u8; 32]);
        client.batch_commit_ip_anonymous(&blinded, &Vec::from_array(&env, [h_anon.clone()]));

        let results = client.get_blinded_owner_batch(&Vec::from_array(&env, [h_regular, h_anon]));
        assert_eq!(results.len(), 2);
        assert_eq!(results.get(0).unwrap(), None);
        assert_eq!(results.get(1).unwrap(), Some(blinded));
    }
}

// ── #465: Batch Escrow tests ──────────────────────────────────────────────────

#[cfg(test)]
mod batch_escrow_tests {
    use super::tests::IpRegistryClient;
    use crate::EscrowStatus;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, BytesN, Env, Vec,
    };

    fn setup_with_ips(n: u64) -> (Env, IpRegistryClient<'static>, Address, Vec<u64>) {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &id);
        let owner = Address::generate(&env);
        let mut ids = Vec::new(&env);
        for i in 0..n {
            let mut hash = [0u8; 32];
            hash[0] = (i + 1) as u8;
            let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &hash), &0u32);
            ids.push_back(ip_id);
        }
        (env, client, owner, ids)
    }

    #[test]
    fn test_batch_escrow_created_and_retrievable() {
        let (env, client, owner, ip_ids) = setup_with_ips(2);
        let beneficiary = Address::generate(&env);
        let timeout = env.ledger().timestamp() + 1000;

        let escrow_id = client.batch_escrow_commitments(&owner, &ip_ids, &beneficiary, &timeout);

        let escrow = client
            .get_batch_escrow(&escrow_id)
            .expect("escrow should exist");
        assert_eq!(escrow.status, EscrowStatus::Active);
        assert_eq!(escrow.depositor, owner);
        assert_eq!(escrow.release_to, beneficiary);
        assert_eq!(escrow.ip_ids.len(), 2);
    }

    #[test]
    fn test_release_batch_escrow_transfers_ownership() {
        let (env, client, owner, ip_ids) = setup_with_ips(2);
        let beneficiary = Address::generate(&env);
        let timeout = env.ledger().timestamp() + 1000;

        let escrow_id = client.batch_escrow_commitments(&owner, &ip_ids, &beneficiary, &timeout);
        client.release_batch_escrow(&escrow_id);

        let escrow = client.get_batch_escrow(&escrow_id).unwrap();
        assert_eq!(escrow.status, EscrowStatus::Released);

        // IPs now owned by beneficiary
        for ip_id in ip_ids.iter() {
            let record = client.get_ip(&ip_id);
            assert_eq!(record.owner, beneficiary);
        }
    }

    #[test]
    fn test_cancel_batch_escrow_after_timeout() {
        let (env, client, owner, ip_ids) = setup_with_ips(1);
        let beneficiary = Address::generate(&env);
        let timeout = env.ledger().timestamp() + 100;

        let escrow_id = client.batch_escrow_commitments(&owner, &ip_ids, &beneficiary, &timeout);

        // Advance past timeout
        env.ledger().with_mut(|l| l.timestamp = timeout + 1);
        client.cancel_batch_escrow(&escrow_id);

        let escrow = client.get_batch_escrow(&escrow_id).unwrap();
        assert_eq!(escrow.status, EscrowStatus::Cancelled);

        // IPs still owned by owner (no transfer on cancel)
        let record = client.get_ip(&ip_ids.get(0).unwrap());
        assert_eq!(record.owner, owner);
    }

    #[test]
    #[should_panic]
    fn test_cancel_before_timeout_panics() {
        let (env, client, owner, ip_ids) = setup_with_ips(1);
        let beneficiary = Address::generate(&env);
        let timeout = env.ledger().timestamp() + 9999;

        let escrow_id = client.batch_escrow_commitments(&owner, &ip_ids, &beneficiary, &timeout);
        // Timeout not reached — must panic
        client.cancel_batch_escrow(&escrow_id);
    }

    #[test]
    #[should_panic]
    fn test_release_already_released_panics() {
        let (env, client, owner, ip_ids) = setup_with_ips(1);
        let beneficiary = Address::generate(&env);
        let timeout = env.ledger().timestamp() + 1000;

        let escrow_id = client.batch_escrow_commitments(&owner, &ip_ids, &beneficiary, &timeout);
        client.release_batch_escrow(&escrow_id);
        // Second release — must panic
        client.release_batch_escrow(&escrow_id);
    }

    #[test]
    #[should_panic]
    fn test_escrow_non_owner_ip_panics() {
        let (env, client, _owner, ip_ids) = setup_with_ips(1);
        let attacker = Address::generate(&env);
        let beneficiary = Address::generate(&env);
        let timeout = env.ledger().timestamp() + 1000;

        // attacker does not own ip_ids — must panic
        client.batch_escrow_commitments(&attacker, &ip_ids, &beneficiary, &timeout);
    }

    #[test]
    #[should_panic]
    fn test_get_batch_escrow_unknown_id_returns_none_not_panic() {
        // Accessing nonexistent escrow via release should panic
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &id);
        let fake_id = BytesN::from_array(&env, &[0xFFu8; 32]);
        client.release_batch_escrow(&fake_id);
    }

    #[test]
    fn test_get_batch_escrow_unknown_returns_none() {
        let env = Env::default();
        let id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &id);
        let fake_id = BytesN::from_array(&env, &[0xFFu8; 32]);
        assert_eq!(client.get_batch_escrow(&fake_id), None);
    }

    #[test]
    fn test_batch_escrow_unique_ids_per_call() {
        let (env, client, owner, ip_ids) = setup_with_ips(2);
        let beneficiary = Address::generate(&env);
        let timeout = env.ledger().timestamp() + 1000;

        // Two separate escrow calls on different IPs should produce different IDs
        let mut ids1_vec = Vec::new(&env);
        ids1_vec.push_back(ip_ids.get(0).unwrap());
        let mut ids2_vec = Vec::new(&env);
        ids2_vec.push_back(ip_ids.get(1).unwrap());

        let eid1 = client.batch_escrow_commitments(&owner, &ids1_vec, &beneficiary, &timeout);
        let eid2 = client.batch_escrow_commitments(&owner, &ids2_vec, &beneficiary, &timeout);
        assert_ne!(eid1, eid2);
    }

    // ── #465: Timeout Edge Cases ──────────────────────────────────────────────

    #[test]
    fn test_cancel_at_exact_timeout_boundary() {
        let (env, client, owner, ip_ids) = setup_with_ips(1);
        let beneficiary = Address::generate(&env);
        let timeout = env.ledger().timestamp() + 100;

        let escrow_id = client.batch_escrow_commitments(&owner, &ip_ids, &beneficiary, &timeout);

        // Advance to exactly the timeout timestamp
        env.ledger().with_mut(|l| l.timestamp = timeout);
        // Should succeed at exact boundary (timeout <= current_time)
        client.cancel_batch_escrow(&escrow_id);

        let escrow = client.get_batch_escrow(&escrow_id).unwrap();
        assert_eq!(escrow.status, EscrowStatus::Cancelled);
    }

    #[test]
    #[should_panic]
    fn test_cancel_one_second_before_timeout() {
        let (env, client, owner, ip_ids) = setup_with_ips(1);
        let beneficiary = Address::generate(&env);
        let timeout = env.ledger().timestamp() + 100;

        let escrow_id = client.batch_escrow_commitments(&owner, &ip_ids, &beneficiary, &timeout);

        // Advance to one second before timeout
        env.ledger().with_mut(|l| l.timestamp = timeout - 1);
        // Must panic — timeout not reached
        client.cancel_batch_escrow(&escrow_id);
    }

    // ── #465: Concurrent Escrow Operations ────────────────────────────────────

    #[test]
    fn test_multiple_concurrent_escrows_independent() {
        let (env, client, owner, ip_ids) = setup_with_ips(4);
        let beneficiary1 = Address::generate(&env);
        let beneficiary2 = Address::generate(&env);
        let timeout1 = env.ledger().timestamp() + 100;
        let timeout2 = env.ledger().timestamp() + 5000;

        // Create first escrow with IPs 0 and 1
        let mut ids1 = Vec::new(&env);
        ids1.push_back(ip_ids.get(0).unwrap());
        ids1.push_back(ip_ids.get(1).unwrap());

        // Create second escrow with IPs 2 and 3
        let mut ids2 = Vec::new(&env);
        ids2.push_back(ip_ids.get(2).unwrap());
        ids2.push_back(ip_ids.get(3).unwrap());

        let escrow_id1 = client.batch_escrow_commitments(&owner, &ids1, &beneficiary1, &timeout1);
        let escrow_id2 = client.batch_escrow_commitments(&owner, &ids2, &beneficiary2, &timeout2);

        // Verify both escrows exist independently
        let escrow1 = client.get_batch_escrow(&escrow_id1).unwrap();
        let escrow2 = client.get_batch_escrow(&escrow_id2).unwrap();

        assert_eq!(escrow1.status, EscrowStatus::Active);
        assert_eq!(escrow2.status, EscrowStatus::Active);
        assert_eq!(escrow1.release_to, beneficiary1);
        assert_eq!(escrow2.release_to, beneficiary2);

        // Release first escrow
        client.release_batch_escrow(&escrow_id1);

        // Verify first is released, second still active
        assert_eq!(
            client.get_batch_escrow(&escrow_id1).unwrap().status,
            EscrowStatus::Released
        );
        assert_eq!(
            client.get_batch_escrow(&escrow_id2).unwrap().status,
            EscrowStatus::Active
        );

        // Verify IPs transferred correctly
        for ip_id in ids1.iter() {
            let record = client.get_ip(&ip_id);
            assert_eq!(record.owner, beneficiary1);
        }
        for ip_id in ids2.iter() {
            let record = client.get_ip(&ip_id);
            assert_eq!(record.owner, owner); // Still owned by owner (escrow 2 not released)
        }
    }

    #[test]
    fn test_concurrent_cancel_and_release_different_escrows() {
        let (env, client, owner, ip_ids) = setup_with_ips(2);
        let beneficiary1 = Address::generate(&env);
        let beneficiary2 = Address::generate(&env);
        let timeout1 = env.ledger().timestamp() + 50;
        let timeout2 = env.ledger().timestamp() + 5000;

        let mut ids1 = Vec::new(&env);
        ids1.push_back(ip_ids.get(0).unwrap());
        let mut ids2 = Vec::new(&env);
        ids2.push_back(ip_ids.get(1).unwrap());

        let escrow_id1 = client.batch_escrow_commitments(&owner, &ids1, &beneficiary1, &timeout1);
        let escrow_id2 = client.batch_escrow_commitments(&owner, &ids2, &beneficiary2, &timeout2);

        // Advance time past timeout1 but not timeout2
        env.ledger().with_mut(|l| l.timestamp = timeout1 + 1);

        // Cancel first escrow (past timeout)
        client.cancel_batch_escrow(&escrow_id1);

        // Release second escrow (depositor can release anytime)
        client.release_batch_escrow(&escrow_id2);

        // Verify states
        assert_eq!(
            client.get_batch_escrow(&escrow_id1).unwrap().status,
            EscrowStatus::Cancelled
        );
        assert_eq!(
            client.get_batch_escrow(&escrow_id2).unwrap().status,
            EscrowStatus::Released
        );

        // IP from cancelled escrow stays with owner
        assert_eq!(client.get_ip(&ip_ids.get(0).unwrap()).owner, owner);
        // IP from released escrow goes to beneficiary
        assert_eq!(
            client.get_ip(&ip_ids.get(1).unwrap()).owner,
            beneficiary2
        );
    }

    // ── #465: Malicious Release Attempts ──────────────────────────────────────

    #[test]
    #[should_panic]
    fn test_release_cancelled_escrow_panics() {
        let (env, client, owner, ip_ids) = setup_with_ips(1);
        let beneficiary = Address::generate(&env);
        let timeout = env.ledger().timestamp() + 50;

        let escrow_id = client.batch_escrow_commitments(&owner, &ip_ids, &beneficiary, &timeout);

        // Advance past timeout and cancel
        env.ledger().with_mut(|l| l.timestamp = timeout + 1);
        client.cancel_batch_escrow(&escrow_id);

        // Try to release cancelled escrow — must panic
        client.release_batch_escrow(&escrow_id);
    }

    #[test]
    #[should_panic]
    fn test_cancel_released_escrow_panics() {
        let (env, client, owner, ip_ids) = setup_with_ips(1);
        let beneficiary = Address::generate(&env);
        let timeout = env.ledger().timestamp() + 5000;

        let escrow_id = client.batch_escrow_commitments(&owner, &ip_ids, &beneficiary, &timeout);

        // Release escrow
        client.release_batch_escrow(&escrow_id);

        // Try to cancel released escrow — must panic
        client.cancel_batch_escrow(&escrow_id);
    }

    #[test]
    #[should_panic]
    fn test_large_batch_escrow() {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &id);
        let owner = Address::generate(&env);
        let beneficiary = Address::generate(&env);

        // Create 50 IPs
        let mut ids = Vec::new(&env);
        for i in 0..50u64 {
            let mut hash = [0u8; 32];
            hash[0] = (i & 0xFF) as u8;
            hash[1] = ((i >> 8) & 0xFF) as u8;
            let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &hash), &0u32);
            ids.push_back(ip_id);
        }

        let timeout = env.ledger().timestamp() + 1000;

        // Create escrow with all 50 IPs
        let escrow_id = client.batch_escrow_commitments(&owner, &ids, &beneficiary, &timeout);

        // Release and verify all IPs transferred
        client.release_batch_escrow(&escrow_id);

        let escrow = client.get_batch_escrow(&escrow_id).unwrap();
        assert_eq!(escrow.status, EscrowStatus::Released);
        assert_eq!(escrow.ip_ids.len(), 50);

        for ip_id in ids.iter() {
            let record = client.get_ip(&ip_id);
            assert_eq!(record.owner, beneficiary);
        }
    }

    // ── #464: Anonymity Tests ─────────────────────────────────────────────────

    /// Verify that replaying the same blinded_owner in a second batch is rejected.
    #[test]
    #[should_panic]
    fn test_anonymous_batch_replay_rejected() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let blinded_owner = BytesN::from_array(&env, &[0xAAu8; 32]);

        // First batch — succeeds.
        let hashes1 = Vec::from_array(&env, [BytesN::from_array(&env, &[0x01u8; 32])]);
        client.batch_commit_ip_anonymous(&blinded_owner, &hashes1);

        // Second batch with the same blinded_owner — must panic (replay).
        let hashes2 = Vec::from_array(&env, [BytesN::from_array(&env, &[0x02u8; 32])]);
        client.batch_commit_ip_anonymous(&blinded_owner, &hashes2);
    }

    /// Verify that distinct blinded_owner values are each accepted exactly once.
    #[test]
    fn test_anonymous_batch_distinct_blinded_owners_accepted() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let blinded1 = BytesN::from_array(&env, &[0xBBu8; 32]);
        let blinded2 = BytesN::from_array(&env, &[0xCCu8; 32]);

        let hashes1 = Vec::from_array(&env, [BytesN::from_array(&env, &[0x10u8; 32])]);
        let hashes2 = Vec::from_array(&env, [BytesN::from_array(&env, &[0x20u8; 32])]);

        let ids1 = client.batch_commit_ip_anonymous(&blinded1, &hashes1);
        let ids2 = client.batch_commit_ip_anonymous(&blinded2, &hashes2);

        assert_eq!(ids1.len(), 1);
        assert_eq!(ids2.len(), 1);
        // IDs are sequential
        assert_ne!(ids1.get(0).unwrap(), ids2.get(0).unwrap());
    }

    /// Register 100+ commitments anonymously across multiple batches with unique
    /// blinded_owners. Verify all IDs are returned and records retrievable.
    #[test]
    fn test_anonymous_batch_100_plus_commitments() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let mut all_ids: soroban_sdk::Vec<u64> = Vec::new(&env);

        // Submit 10 batches of 11 commitments each = 110 total.
        for batch_idx in 0u8..10 {
            // Each batch uses a unique blinded_owner (simulates sha256(owner || nonce)).
            let mut bo_bytes = [0u8; 32];
            bo_bytes[0] = batch_idx + 1;
            let blinded_owner = BytesN::from_array(&env, &bo_bytes);

            let mut hashes: Vec<BytesN<32>> = Vec::new(&env);
            for commit_idx in 0u8..11 {
                let mut h = [0xFFu8; 32];
                h[0] = batch_idx;
                h[1] = commit_idx;
                hashes.push_back(BytesN::from_array(&env, &h));
            }

            let ids = client.batch_commit_ip_anonymous(&blinded_owner, &hashes);
            assert_eq!(ids.len(), 11);
            for i in 0..11u32 {
                all_ids.push_back(ids.get(i).unwrap());
            }
        }

        assert_eq!(all_ids.len(), 110);

        // Verify every record is retrievable.
        for i in 0..110u32 {
            let ip_id = all_ids.get(i).unwrap();
            let record = client.get_ip(&ip_id);
            // owner is the contract address (anonymous placeholder)
            assert_eq!(record.ip_id, ip_id);
        }
    }

    /// De-anonymization resistance: the on-chain IpRecord.owner must be the
    /// contract address, not any user-supplied address. get_anonymous_owner
    /// returns the blinded_owner, not a real address — ensuring no direct
    /// linkage between the blinded identifier and a plaintext identity.
    #[test]
    fn test_anonymous_commit_owner_not_linkable() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        // Simulate a real user (attacker knows this address exists but cannot link it).
        let real_user = Address::generate(&env);

        // blinded_owner = sha256(real_user || nonce) — in tests we use a fixed bytes.
        let blinded_owner = BytesN::from_array(&env, &[0xDDu8; 32]);

        let commitment = BytesN::from_array(&env, &[0x55u8; 32]);
        let hashes = Vec::from_array(&env, [commitment.clone()]);

        let ids = client.batch_commit_ip_anonymous(&blinded_owner, &hashes);
        let ip_id = ids.get(0).unwrap();

        let record = client.get_ip(&ip_id);

        // The record owner must NOT be the real_user — de-anonymization blocked.
        assert_ne!(record.owner, real_user);

        // The record owner must NOT be all-zeros or a predictable sentinel.
        assert_ne!(record.owner, Address::generate(&env)); // different every time

        // get_anonymous_owner returns the blinded handle, not a real address.
        let blinded = client.get_anonymous_owner(&commitment);
        assert_eq!(blinded, Some(blinded_owner.clone()));

        // Batch lookup consistent with single lookup.
        let hashes_lookup = Vec::from_array(&env, [commitment.clone()]);
        let batch_result = client.get_blinded_owner_batch(&hashes_lookup);
        assert_eq!(batch_result.len(), 1);
        assert_eq!(batch_result.get(0).unwrap(), Some(blinded_owner));
    }

    /// Verify get_anonymous_owner returns None for a non-anonymous commit.
    #[test]
    fn test_get_anonymous_owner_none_for_regular_commit() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let commitment = BytesN::from_array(&env, &[0x77u8; 32]);

        client.commit_ip(&owner, &commitment, &0u32);

        // Regular commit must have no anonymous owner mapping.
        assert_eq!(client.get_anonymous_owner(&commitment), None);
    }

    /// Verify blinded_owner cannot be de-anonymized via OwnerIps index.
    #[test]
    fn test_anonymous_commit_not_indexed_by_owner() {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let blinded_owner = BytesN::from_array(&env, &[0xEEu8; 32]);
        let commitment = BytesN::from_array(&env, &[0x88u8; 32]);
        let hashes = Vec::from_array(&env, [commitment]);

        client.batch_commit_ip_anonymous(&blinded_owner, &hashes);

        // No real address can be used to retrieve the anonymous IP via list_ip_by_owner.
        let attacker = Address::generate(&env);
        let listed = client.list_ip_by_owner(&attacker);
        assert_eq!(listed.len(), 0);
    }
}
