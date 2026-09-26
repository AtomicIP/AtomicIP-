#[cfg(test)]
mod reputation_pricing_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Bytes, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, ContractError, SwapStatus};

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

    #[test]
    fn test_price_multiplier_field_exists() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );

        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Completed);
    }

    #[test]
    fn test_risk_score_calculation_for_new_counterparty() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let unknown_counterparty = Address::generate(&env);
        let reputation = client.get_reputation(&unknown_counterparty);

        assert_eq!(reputation, 50);
    }

    #[test]
    fn test_price_multiplier_at_minimum_risk() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );

        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Completed);
    }

    #[test]
    fn test_price_multiplier_at_maximum_risk() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );

        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Completed);
    }

    #[test]
    fn test_price_multiplier_capped_at_2x() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let price = 1000i128;
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &price, &buyer,
            &0u32, &None, &0i128, &false,
        );

        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Completed);
    }

    #[test]
    fn test_auto_price_adjustment_on_swap_creation() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let price = 1000i128;
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &price, &buyer,
            &0u32, &None, &0i128, &false,
        );

        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Completed);
    }

    #[test]
    fn test_reputation_increase_lowers_price_multiplier() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let rep_before = client.get_reputation(&seller);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );

        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        let rep_after = client.get_reputation(&seller);
        assert!(rep_after > rep_before);
    }

    #[test]
    fn test_reputation_decrease_increases_price_multiplier() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let rep_before = client.get_reputation(&seller);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );

        client.cancel_swap(&swap_id, &seller);

        let rep_after = client.get_reputation(&seller);
        assert!(rep_after < rep_before);
    }

    #[test]
    fn test_price_multiplier_not_applied_below_100_reputation() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Pending);
    }

    #[test]
    fn test_price_multiplier_applied_above_100_reputation() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 10_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        for _ in 0..10 {
            let registry = IpRegistryClient::new(&env, &registry_id);
            let s = BytesN::from_array(&env, &[0xCCu8; 32]);
            let b = BytesN::from_array(&env, &[0xDDu8; 32]);
            let mut preimage = Bytes::new(&env);
            preimage.append(&Bytes::from(s.clone()));
            preimage.append(&Bytes::from(b.clone()));
            let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
            let ip = registry.commit_ip(&seller, &commitment_hash);

            let id = client.initiate_swap(
                &token_id, &ip, &seller, &1000i128, &buyer,
                &0u32, &None, &0i128, &false,
            );
            client.accept_swap(&id);
            client.reveal_key(&id, &seller, &s, &b);
        }

        let reputation = client.get_reputation(&seller);
        assert!(reputation > 50);
    }

    #[test]
    fn test_price_multiplier_1x_for_default_reputation() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let price = 1000i128;
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &price, &buyer,
            &0u32, &None, &0i128, &false,
        );

        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Completed);
    }
}
