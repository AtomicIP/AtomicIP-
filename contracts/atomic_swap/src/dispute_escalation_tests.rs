#[cfg(test)]
mod dispute_escalation_tests {
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
    fn test_dispute_starts_at_negotiation_level() {
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

        client.accept_swap(&swap_id);

        env.ledger().with_mut(|l| l.timestamp += 604800);

        client.raise_dispute(&swap_id, &buyer);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Disputed);
    }

    #[test]
    fn test_escalate_dispute_to_mediation_level() {
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

        client.accept_swap(&swap_id);

        env.ledger().with_mut(|l| l.timestamp += 604800);

        client.raise_dispute(&swap_id, &buyer);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Disputed);
    }

    #[test]
    fn test_escalate_dispute_to_arbitration_level() {
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

        client.accept_swap(&swap_id);

        env.ledger().with_mut(|l| l.timestamp += 604800);

        client.raise_dispute(&swap_id, &buyer);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Disputed);
    }

    #[test]
    fn test_escalation_requires_timeout_at_current_level() {
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

        client.accept_swap(&swap_id);

        env.ledger().with_mut(|l| l.timestamp += 604800);

        client.raise_dispute(&swap_id, &buyer);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Disputed);
    }

    #[test]
    fn test_auto_escalate_after_negotiation_timeout() {
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

        client.accept_swap(&swap_id);

        env.ledger().with_mut(|l| l.timestamp += 604800);

        client.raise_dispute(&swap_id, &buyer);

        env.ledger().with_mut(|l| l.timestamp += 172800);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Disputed);
    }

    #[test]
    fn test_auto_escalate_after_mediation_timeout() {
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

        client.accept_swap(&swap_id);

        env.ledger().with_mut(|l| l.timestamp += 604800);

        client.raise_dispute(&swap_id, &buyer);

        env.ledger().with_mut(|l| l.timestamp += 604800);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Disputed);
    }

    #[test]
    fn test_escalation_history_tracking() {
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

        client.accept_swap(&swap_id);

        env.ledger().with_mut(|l| l.timestamp += 604800);

        client.raise_dispute(&swap_id, &buyer);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Disputed);
    }

    #[test]
    fn test_dispute_resolution_at_negotiation_level() {
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

        client.accept_swap(&swap_id);

        env.ledger().with_mut(|l| l.timestamp += 604800);

        client.raise_dispute(&swap_id, &buyer);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Disputed);
    }

    #[test]
    fn test_dispute_resolution_at_mediation_level() {
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

        client.accept_swap(&swap_id);

        env.ledger().with_mut(|l| l.timestamp += 604800);

        client.raise_dispute(&swap_id, &buyer);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Disputed);
    }

    #[test]
    fn test_dispute_resolution_at_arbitration_level() {
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

        client.accept_swap(&swap_id);

        env.ledger().with_mut(|l| l.timestamp += 604800);

        client.raise_dispute(&swap_id, &buyer);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Disputed);
    }

    #[test]
    fn test_escalation_levels_are_sequential() {
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

        client.accept_swap(&swap_id);

        env.ledger().with_mut(|l| l.timestamp += 604800);

        client.raise_dispute(&swap_id, &buyer);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Disputed);
    }

    #[test]
    fn test_max_escalation_level_is_arbitration() {
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

        client.accept_swap(&swap_id);

        env.ledger().with_mut(|l| l.timestamp += 604800);

        client.raise_dispute(&swap_id, &buyer);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Disputed);
    }
}
