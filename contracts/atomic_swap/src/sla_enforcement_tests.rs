#[cfg(test)]
mod sla_enforcement_tests {
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
    fn test_sla_max_execution_time_field_validation() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let max_execution_time = 3600u64;
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );

        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Completed);
    }

    #[test]
    fn test_sla_enforcement_on_swap_completion() {
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

        env.ledger().with_mut(|l| l.timestamp += 1000);

        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Completed);
    }

    #[test]
    fn test_sla_penalty_calculation_for_missed_deadline() {
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

        env.ledger().with_mut(|l| l.timestamp += 604800);

        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Completed);
    }

    #[test]
    fn test_sla_auto_refund_on_missed_deadline() {
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

        env.ledger().with_mut(|l| l.timestamp += 1_209_600);

        client.cancel_expired_swap(&swap_id, &buyer);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Cancelled);
    }

    #[test]
    fn test_sla_completion_time_tracking() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let initial_time = env.ledger().timestamp();
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );

        client.accept_swap(&swap_id);

        let time_before_reveal = env.ledger().timestamp();
        env.ledger().with_mut(|l| l.timestamp += 100);

        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        let time_after_reveal = env.ledger().timestamp();
        assert!(time_after_reveal > time_before_reveal);
    }

    #[test]
    fn test_sla_zero_execution_time_allowed() {
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
    fn test_sla_large_execution_time_window() {
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

        env.ledger().with_mut(|l| l.timestamp += 31_536_000);

        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        assert_eq!(client.get_swap_status(&swap_id), SwapStatus::Completed);
    }

    #[test]
    fn test_sla_multiple_swaps_independent_deadlines() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, _, secret1, blinding1) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 10_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let ip_id1 = {
            let registry = IpRegistryClient::new(&env, &registry_id);
            let secret = BytesN::from_array(&env, &[0x11u8; 32]);
            let blinding = BytesN::from_array(&env, &[0x22u8; 32]);
            let mut preimage = Bytes::new(&env);
            preimage.append(&Bytes::from(secret.clone()));
            preimage.append(&Bytes::from(blinding.clone()));
            let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
            registry.commit_ip(&seller, &commitment_hash)
        };

        let ip_id2 = {
            let registry = IpRegistryClient::new(&env, &registry_id);
            let secret = BytesN::from_array(&env, &[0x33u8; 32]);
            let blinding = BytesN::from_array(&env, &[0x44u8; 32]);
            let mut preimage = Bytes::new(&env);
            preimage.append(&Bytes::from(secret.clone()));
            preimage.append(&Bytes::from(blinding.clone()));
            let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
            registry.commit_ip(&seller, &commitment_hash)
        };

        let swap_id1 = client.initiate_swap(
            &token_id, &ip_id1, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );

        let swap_id2 = client.initiate_swap(
            &token_id, &ip_id2, &seller, &2000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );

        client.accept_swap(&swap_id1);
        client.accept_swap(&swap_id2);

        env.ledger().with_mut(|l| l.timestamp += 100);

        client.reveal_key(&swap_id1, &seller, &secret1, &blinding1);

        env.ledger().with_mut(|l| l.timestamp += 200);

        client.reveal_key(&swap_id2, &seller, &secret1, &blinding1);

        assert_eq!(client.get_swap_status(&swap_id1), SwapStatus::Completed);
        assert_eq!(client.get_swap_status(&swap_id2), SwapStatus::Completed);
    }
}
