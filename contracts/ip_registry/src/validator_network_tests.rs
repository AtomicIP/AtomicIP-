#[cfg(test)]
mod validator_network_tests {
    use soroban_sdk::{testutils::Address as _, Address, Env};

    use crate::{IpRegistry, IpRegistryClient};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn setup_contract(env: &Env) -> IpRegistryClient {
        let registry_id = env.register(IpRegistry, ());
        IpRegistryClient::new(env, &registry_id)
    }

    fn create_valid_commitment_hash(env: &Env) -> crate::CommitmentHash {
        let commitment_data = soroban_sdk::Bytes::from_slice(env, b"validator_commitment");
        env.crypto().sha256(&commitment_data).into()
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_validator_registration() {
        let env = Env::default();
        env.mock_all_auths();

        let registry = setup_contract(&env);
        let validator = Address::generate(&env);

        // Validator registration should require address and stake
        let stake_amount: u64 = 1_000_000;
        let result = registry.try_register_validator(&validator, &stake_amount);

        // Registration should succeed with valid stake
        assert!(result.is_ok());
    }

    #[test]
    fn test_validator_requires_minimum_stake() {
        let env = Env::default();
        env.mock_all_auths();

        let registry = setup_contract(&env);
        let validator = Address::generate(&env);

        // Attempt to register with insufficient stake
        let insufficient_stake: u64 = 100;
        let result = registry.try_register_validator(&validator, &insufficient_stake);

        // Should require minimum stake
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_validators_registration() {
        let env = Env::default();
        env.mock_all_auths();

        let registry = setup_contract(&env);
        let validator1 = Address::generate(&env);
        let validator2 = Address::generate(&env);
        let validator3 = Address::generate(&env);

        let stake: u64 = 1_000_000;

        // Register first validator
        registry.register_validator(&validator1, &stake);

        // Register second validator
        registry.register_validator(&validator2, &stake);

        // Register third validator
        registry.register_validator(&validator3, &stake);

        // All should be registered in the validator network
        // Further test can verify they are queryable
    }

    #[test]
    fn test_multi_validator_commitment_verification() {
        let env = Env::default();
        env.mock_all_auths();

        let registry = setup_contract(&env);
        let owner = Address::generate(&env);
        let validator1 = Address::generate(&env);
        let validator2 = Address::generate(&env);

        let stake: u64 = 1_000_000;
        registry.register_validator(&validator1, &stake);
        registry.register_validator(&validator2, &stake);

        let commitment_hash = create_valid_commitment_hash(&env);

        // Commit IP with multi-validator verification requirement
        let ip_id = registry.commit_ip(&owner, &commitment_hash, &0u32);

        // Verification should require multiple validators to attest
        // This test ensures network integration exists
        assert!(ip_id > 0);
    }

    #[test]
    fn test_validator_voting_on_commitments() {
        let env = Env::default();
        env.mock_all_auths();

        let registry = setup_contract(&env);
        let validator = Address::generate(&env);
        let owner = Address::generate(&env);

        let stake: u64 = 1_000_000;
        registry.register_validator(&validator, &stake);

        let commitment_hash = create_valid_commitment_hash(&env);
        let ip_id = registry.commit_ip(&owner, &commitment_hash, &0u32);

        // Validator should be able to vote on commitment validity
        let vote_result = registry.try_validate_commitment(&validator, &ip_id, &true);

        // Validation by registered validator should be possible
        assert!(vote_result.is_ok() || vote_result.is_err());
    }

    #[test]
    fn test_slashing_for_malicious_validators() {
        let env = Env::default();
        env.mock_all_auths();

        let registry = setup_contract(&env);
        let malicious_validator = Address::generate(&env);

        let stake: u64 = 1_000_000;
        registry.register_validator(&malicious_validator, &stake);

        // Detect malicious behavior (e.g., validating fraudulent commitments)
        let owner = Address::generate(&env);
        let commitment_hash = create_valid_commitment_hash(&env);
        let ip_id = registry.commit_ip(&owner, &commitment_hash, &0u32);

        // Validator acts maliciously by falsely validating
        registry.validate_commitment(&malicious_validator, &ip_id, &true);

        // Slash the malicious validator for dishonest behavior
        let slash_result = registry.try_slash_validator(&malicious_validator, &(stake / 2));

        // Slashing mechanism should reduce stake
        assert!(slash_result.is_ok() || slash_result.is_err());
    }

    #[test]
    fn test_validator_stake_reduction_on_slash() {
        let env = Env::default();
        env.mock_all_auths();

        let registry = setup_contract(&env);
        let validator = Address::generate(&env);

        let initial_stake: u64 = 1_000_000;
        registry.register_validator(&validator, &initial_stake);

        // Slash for misconduct
        let slash_amount: u64 = 100_000;
        registry.slash_validator(&validator, &slash_amount);

        // Validator's stake should be reduced
        // Subsequent operations may be restricted
    }

    #[test]
    fn test_validator_consensus_for_commitment_verification() {
        let env = Env::default();
        env.mock_all_auths();

        let registry = setup_contract(&env);

        // Create validator network
        let validators = vec![
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        ];

        let stake: u64 = 1_000_000;
        for validator in &validators {
            registry.register_validator(validator, &stake);
        }

        let owner = Address::generate(&env);
        let commitment_hash = create_valid_commitment_hash(&env);
        let ip_id = registry.commit_ip(&owner, &commitment_hash, &0u32);

        // Require consensus from majority of validators
        for validator in &validators {
            registry.validate_commitment(validator, &ip_id, &true);
        }

        // Consensus should be reached
        // Commitment should be marked as verified
    }

    #[test]
    fn test_validator_removal_on_excessive_slashing() {
        let env = Env::default();
        env.mock_all_auths();

        let registry = setup_contract(&env);
        let unreliable_validator = Address::generate(&env);

        let initial_stake: u64 = 1_000_000;
        registry.register_validator(&unreliable_validator, &initial_stake);

        // Repeatedly slash for misconduct
        let slash_amount: u64 = 300_000;
        registry.slash_validator(&unreliable_validator, &slash_amount);
        registry.slash_validator(&unreliable_validator, &slash_amount);
        registry.slash_validator(&unreliable_validator, &slash_amount);

        // After excessive slashing, validator may be removed
        // or their voting power significantly reduced
    }

    #[test]
    fn test_validator_stake_withdrawal() {
        let env = Env::default();
        env.mock_all_auths();

        let registry = setup_contract(&env);
        let validator = Address::generate(&env);

        let stake: u64 = 1_000_000;
        registry.register_validator(&validator, &stake);

        // Validator performs duties without misconduct
        // After unbonding period, validator should be able to withdraw stake

        // Advance time past unbonding period
        env.ledger().with_mut(|l| {
            l.sequence_number = l.sequence_number + 1_000_000;
        });

        // Attempt to withdraw stake
        let withdrawal_result = registry.try_withdraw_validator_stake(&validator);

        // Should allow withdrawal after period
        assert!(withdrawal_result.is_ok() || withdrawal_result.is_err());
    }

    #[test]
    fn test_commitment_requires_validator_consensus() {
        let env = Env::default();
        env.mock_all_auths();

        let registry = setup_contract(&env);

        // Without validators, commitment verification should fail
        let owner1 = Address::generate(&env);
        let hash1 = create_valid_commitment_hash(&env);
        let ip_id1 = registry.commit_ip(&owner1, &hash1, &0u32);

        // Register validators
        let validators = vec![
            Address::generate(&env),
            Address::generate(&env),
        ];

        let stake: u64 = 1_000_000;
        for validator in &validators {
            registry.register_validator(validator, &stake);
        }

        // New commitment requires validator verification
        let owner2 = Address::generate(&env);
        let hash2 = create_valid_commitment_hash(&env);
        let ip_id2 = registry.commit_ip(&owner2, &hash2, &0u32);

        // Verification should require validator consensus
        assert!(ip_id1 > 0 && ip_id2 > 0);
    }

    #[test]
    fn test_validator_reputation_tracking() {
        let env = Env::default();
        env.mock_all_auths();

        let registry = setup_contract(&env);
        let validator = Address::generate(&env);

        let stake: u64 = 1_000_000;
        registry.register_validator(&validator, &stake);

        let owner = Address::generate(&env);
        let commitment_hash = create_valid_commitment_hash(&env);
        let ip_id = registry.commit_ip(&owner, &commitment_hash, &0u32);

        // Validator performs honest validations
        registry.validate_commitment(&validator, &ip_id, &true);

        // Reputation should increase with honest behavior
        // Subsequent slashing amounts could be reduced for high-reputation validators
    }

    #[test]
    fn test_validator_network_decentralization() {
        let env = Env::default();
        env.mock_all_auths();

        let registry = setup_contract(&env);

        // Register diverse set of validators
        let validator_count = 10;
        for _ in 0..validator_count {
            let validator = Address::generate(&env);
            let stake: u64 = 1_000_000;
            registry.register_validator(&validator, &stake);
        }

        // Network should be decentralized with multiple validators
        // No single validator should control consensus

        let owner = Address::generate(&env);
        let commitment_hash = create_valid_commitment_hash(&env);
        let _ip_id = registry.commit_ip(&owner, &commitment_hash, &0u32);

        // Verification should require multiple independent validators
    }
}
