/// Comprehensive tests for #468: Swap Conditional Completion.
///
/// Covers:
/// - accept_swap_conditional with no conditions (unconditional)
/// - accept_swap_conditional with KeyValid condition
/// - accept_swap_conditional with PriceBelow condition (pass and fail)
/// - accept_swap_conditional with TimeAfter condition (pass and fail)
/// - Multiple conditions (all pass, one fails)
/// - Conditions re-evaluated at reveal_key time
#[cfg(test)]
mod conditional_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        vec, Address, Bytes, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, ContractError, SwapCondition, SwapStatus};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn setup_registry(env: &Env, owner: &Address) -> (Address, u64, BytesN<32>, BytesN<32>) {
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);

        let secret = BytesN::from_array(env, &[0xAAu8; 32]);
        let blinding = BytesN::from_array(env, &[0xBBu8; 32]);

        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(secret.clone()));
        preimage.append(&Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

        let ip_id = registry.commit_ip(owner, &commitment_hash);
        (registry_id, ip_id, secret, blinding)
    }

    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    fn setup_swap_contract(env: &Env, registry_id: &Address) -> Address {
        let contract_id = env.register(AtomicSwap, ());
        AtomicSwapClient::new(env, &contract_id).initialize(registry_id);
        contract_id
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    /// accept_swap_conditional with an empty conditions list behaves like accept_swap.
    #[test]
    fn test_conditional_no_conditions_succeeds() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        let conditions = vec![&env];
        client.accept_swap_conditional(&swap_id, &conditions);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Accepted);
        assert!(swap.conditions.is_empty());
    }

    /// accept_swap_conditional with KeyValid condition: swap is accepted and
    /// reveal_key with a valid key completes the swap.
    #[test]
    fn test_conditional_key_valid_reveal_succeeds() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        let conditions = vec![&env, SwapCondition::KeyValid];
        client.accept_swap_conditional(&swap_id, &conditions);

        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Accepted
        );

        // Valid key → swap completes
        client.reveal_key(&swap_id, &seller, &secret, &blinding);
        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Completed
        );
    }

    /// PriceBelow condition passes when swap price is below the threshold.
    #[test]
    fn test_conditional_price_below_passes() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, _secret, _blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        // price=300, threshold=500 → 300 < 500 → passes
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &300_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        let conditions = vec![&env, SwapCondition::PriceBelow(500)];
        client.accept_swap_conditional(&swap_id, &conditions);

        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Accepted
        );
    }

    /// PriceBelow condition fails when swap price meets or exceeds the threshold.
    #[test]
    fn test_conditional_price_below_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, _secret, _blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        // price=500, threshold=500 → 500 < 500 is false → fails
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        let conditions = vec![&env, SwapCondition::PriceBelow(500)];
        let result = client.try_accept_swap_conditional(&swap_id, &conditions);
        assert_eq!(
            result.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(ContractError::ConditionNotMet as u32)
        );
    }

    /// TimeAfter condition passes when current ledger time is at or after the threshold.
    #[test]
    fn test_conditional_time_after_passes() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, _secret, _blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        // Advance time past the threshold
        let threshold = env.ledger().timestamp() + 100;
        env.ledger().with_mut(|l| l.timestamp = threshold);

        let conditions = vec![&env, SwapCondition::TimeAfter(threshold)];
        client.accept_swap_conditional(&swap_id, &conditions);

        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Accepted
        );
    }

    /// TimeAfter condition fails when current ledger time is before the threshold.
    #[test]
    fn test_conditional_time_after_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, _secret, _blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        // Threshold is in the future
        let future_ts = env.ledger().timestamp() + 9999;
        let conditions = vec![&env, SwapCondition::TimeAfter(future_ts)];
        let result = client.try_accept_swap_conditional(&swap_id, &conditions);
        assert_eq!(
            result.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(ContractError::ConditionNotMet as u32)
        );
    }

    /// Multiple conditions: all pass → swap accepted.
    #[test]
    fn test_conditional_multiple_all_pass() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &300_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        let now = env.ledger().timestamp();
        // PriceBelow(500): 300 < 500 ✓, TimeAfter(now): now >= now ✓, KeyValid: deferred ✓
        let conditions = vec![
            &env,
            SwapCondition::PriceBelow(500),
            SwapCondition::TimeAfter(now),
            SwapCondition::KeyValid,
        ];
        client.accept_swap_conditional(&swap_id, &conditions);

        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Accepted
        );

        // Reveal with valid key → completes
        client.reveal_key(&swap_id, &seller, &secret, &blinding);
        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Completed
        );
    }

    /// Multiple conditions: one fails → ConditionNotMet, swap stays Pending.
    #[test]
    fn test_conditional_multiple_one_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, _secret, _blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &300_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        // PriceBelow(500) passes, but TimeAfter(future) fails
        let future_ts = env.ledger().timestamp() + 9999;
        let conditions = vec![
            &env,
            SwapCondition::PriceBelow(500),
            SwapCondition::TimeAfter(future_ts),
        ];
        let result = client.try_accept_swap_conditional(&swap_id, &conditions);
        assert_eq!(
            result.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(ContractError::ConditionNotMet as u32)
        );

        // Swap must still be Pending
        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Pending
        );
    }

    /// Conditions stored on the swap record are re-evaluated at reveal_key time.
    /// A TimeAfter condition that was satisfied at accept time is still satisfied at reveal.
    #[test]
    fn test_conditional_conditions_persist_to_reveal() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &300_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        let now = env.ledger().timestamp();
        let conditions = vec![&env, SwapCondition::PriceBelow(500), SwapCondition::TimeAfter(now)];
        client.accept_swap_conditional(&swap_id, &conditions);

        // Advance time and reveal — conditions still hold
        env.ledger().with_mut(|l| l.timestamp = now + 100);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Completed
        );
    }

    /// accept_swap_conditional on a non-Pending swap returns NotPending.
    #[test]
    fn test_conditional_on_accepted_swap_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, _secret, _blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        // Accept normally first
        client.accept_swap(&swap_id);

        // Trying to accept_swap_conditional on an already-Accepted swap must fail
        let conditions = vec![&env];
        let result = client.try_accept_swap_conditional(&swap_id, &conditions);
        assert!(result.is_err());
    }
}
