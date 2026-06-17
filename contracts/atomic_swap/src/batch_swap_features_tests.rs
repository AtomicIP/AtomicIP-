#[cfg(test)]
#[allow(clippy::module_inception)]
mod batch_swap_features_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _, token::StellarAssetClient, Address, Bytes, BytesN, Env, Vec,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn setup_registry(env: &Env, _owner: &Address) -> Address {
        let registry_id = env.register(IpRegistry, ());
        let _ = IpRegistryClient::new(env, &registry_id);
        registry_id
    }

    fn commit_ip(
        env: &Env,
        registry_id: &Address,
        owner: &Address,
        seed: u8,
    ) -> (u64, BytesN<32>, BytesN<32>) {
        let registry = IpRegistryClient::new(env, registry_id);
        let secret = BytesN::from_array(env, &[seed; 32]);
        let blinding = BytesN::from_array(env, &[seed.wrapping_add(0x80); 32]);
        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(secret.clone()));
        preimage.append(&Bytes::from(blinding.clone()));
        let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(owner, &hash, &0u32);
        (ip_id, secret, blinding)
    }

    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    fn setup_contract(env: &Env, registry_id: &Address) -> Address {
        let contract_id = env.register(AtomicSwap, ());
        AtomicSwapClient::new(env, &contract_id).initialize(registry_id);
        contract_id
    }

    // ── Reputation: batch_reveal_keys updates reputation ─────────────────────

    #[test]
    fn test_batch_reveal_keys_updates_reputation() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, s1, b1) = commit_ip(&env, &registry_id, &seller, 0x01);
        let (ip2, s2, b2) = commit_ip(&env, &registry_id, &seller, 0x02);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);
        ip_ids.push_back(ip2);

        let mut prices = Vec::new(&env);
        prices.push_back(1000i128);
        prices.push_back(2000i128);

        let swap_ids =
            client.batch_initiate_swap(&token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None);

        let mut ids = Vec::new(&env);
        ids.push_back(swap_ids.get(0).unwrap());
        ids.push_back(swap_ids.get(1).unwrap());
        client.batch_accept_swaps(&ids, &buyer);

        let mut secrets = Vec::new(&env);
        secrets.push_back(s1);
        secrets.push_back(s2);

        let mut blindings = Vec::new(&env);
        blindings.push_back(b1);
        blindings.push_back(b2);

        // Before reveal: default reputation = 50
        assert_eq!(client.get_reputation(&seller), 50);
        assert_eq!(client.get_reputation(&buyer), 50);

        client.batch_reveal_keys(&ids, &secrets, &blindings, &seller);

        // After 2 completions: 50 + 2*5 = 60
        assert_eq!(client.get_reputation(&seller), 60);
        assert_eq!(client.get_reputation(&buyer), 60);
    }

    #[test]
    fn test_batch_reveal_keys_single_swap_reputation() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, s1, b1) = commit_ip(&env, &registry_id, &seller, 0x10);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);
        let mut prices = Vec::new(&env);
        prices.push_back(500i128);

        let swap_ids =
            client.batch_initiate_swap(&token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None);

        let mut ids = Vec::new(&env);
        ids.push_back(swap_ids.get(0).unwrap());
        client.batch_accept_swaps(&ids, &buyer);

        let mut secrets = Vec::new(&env);
        secrets.push_back(s1);
        let mut blindings = Vec::new(&env);
        blindings.push_back(b1);

        client.batch_reveal_keys(&ids, &secrets, &blindings, &seller);

        // 50 + 5 = 55
        assert_eq!(client.get_reputation(&seller), 55);
        assert_eq!(client.get_reputation(&buyer), 55);
    }

    // ── Insurance: batch_accept_swaps collects insurance premiums ────────────

    #[test]
    fn test_batch_accept_swaps_collects_insurance_premium() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        // Mint enough for price + insurance premium (2% of price)
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, _, _) = commit_ip(&env, &registry_id, &seller, 0x20);
        let (ip2, _, _) = commit_ip(&env, &registry_id, &seller, 0x21);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);
        ip_ids.push_back(ip2);
        let mut prices = Vec::new(&env);
        prices.push_back(1000i128);
        prices.push_back(2000i128);

        // Initiate with insurance enabled
        let swap_ids = client.batch_initiate_swap_insurance(
            &token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None, &true,
        );

        let mut ids = Vec::new(&env);
        ids.push_back(swap_ids.get(0).unwrap());
        ids.push_back(swap_ids.get(1).unwrap());

        // Accept — should collect premiums (20 + 40 = 60) into pool
        client.batch_accept_swaps(&ids, &buyer);

        // Verify swaps are Accepted
        assert_eq!(
            client.get_swap(&ids.get(0).unwrap()).unwrap().status,
            SwapStatus::Accepted
        );
        assert_eq!(
            client.get_swap(&ids.get(1).unwrap()).unwrap().status,
            SwapStatus::Accepted
        );
    }

    #[test]
    fn test_batch_accept_swaps_no_insurance_no_premium() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, _, _) = commit_ip(&env, &registry_id, &seller, 0x30);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);
        let mut prices = Vec::new(&env);
        prices.push_back(1000i128);

        // No insurance
        let swap_ids =
            client.batch_initiate_swap(&token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None);

        let mut ids = Vec::new(&env);
        ids.push_back(swap_ids.get(0).unwrap());
        client.batch_accept_swaps(&ids, &buyer);

        let swap = client.get_swap(&ids.get(0).unwrap()).unwrap();
        assert_eq!(swap.status, SwapStatus::Accepted);
        assert!(!swap.insurance_enabled);
        assert_eq!(swap.insurance_premium, 0);
    }

    // ── Arbitration: batch_arbitrate_swaps ────────────────────────────────────

    #[test]
    fn test_batch_arbitrate_swaps_refund() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let arbitrator = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, _, _) = commit_ip(&env, &registry_id, &seller, 0x40);
        let (ip2, _, _) = commit_ip(&env, &registry_id, &seller, 0x41);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);
        ip_ids.push_back(ip2);
        let mut prices = Vec::new(&env);
        prices.push_back(1000i128);
        prices.push_back(2000i128);

        let swap_ids =
            client.batch_initiate_swap(&token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None);

        let mut ids = Vec::new(&env);
        ids.push_back(swap_ids.get(0).unwrap());
        ids.push_back(swap_ids.get(1).unwrap());
        client.batch_accept_swaps(&ids, &buyer);

        // Raise disputes
        client.raise_dispute(&ids.get(0).unwrap());
        client.raise_dispute(&ids.get(1).unwrap());

        // Batch arbitrate — refund both
        client.batch_arbitrate_swaps(&ids, &arbitrator, &true);

        assert_eq!(
            client.get_swap(&ids.get(0).unwrap()).unwrap().status,
            SwapStatus::Cancelled
        );
        assert_eq!(
            client.get_swap(&ids.get(1).unwrap()).unwrap().status,
            SwapStatus::Cancelled
        );
    }

    #[test]
    fn test_batch_arbitrate_swaps_complete() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let arbitrator = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, _, _) = commit_ip(&env, &registry_id, &seller, 0x50);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);
        let mut prices = Vec::new(&env);
        prices.push_back(1000i128);

        let swap_ids =
            client.batch_initiate_swap(&token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None);

        let mut ids = Vec::new(&env);
        ids.push_back(swap_ids.get(0).unwrap());
        client.batch_accept_swaps(&ids, &buyer);
        client.raise_dispute(&ids.get(0).unwrap());

        // Batch arbitrate — complete (no refund)
        client.batch_arbitrate_swaps(&ids, &arbitrator, &false);

        assert_eq!(
            client.get_swap(&ids.get(0).unwrap()).unwrap().status,
            SwapStatus::Completed
        );
    }

    // ── Escrow: batch_escrow_deposit ──────────────────────────────────────────

    #[test]
    fn test_batch_escrow_deposit_moves_to_accepted() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, _, _) = commit_ip(&env, &registry_id, &seller, 0x60);
        let (ip2, _, _) = commit_ip(&env, &registry_id, &seller, 0x61);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);
        ip_ids.push_back(ip2);
        let mut prices = Vec::new(&env);
        prices.push_back(500i128);
        prices.push_back(800i128);
        let timeout = env.ledger().timestamp() + 3600;
        let mut timeouts = Vec::new(&env);
        timeouts.push_back(timeout);
        timeouts.push_back(timeout);

        let swap_ids =
            client.batch_initiate_escrow(&token_id, &ip_ids, &seller, &prices, &buyer, &timeouts);

        // Both should be Pending
        assert_eq!(
            client.get_swap(&swap_ids.get(0).unwrap()).unwrap().status,
            SwapStatus::Pending
        );
        assert_eq!(
            client.get_swap(&swap_ids.get(1).unwrap()).unwrap().status,
            SwapStatus::Pending
        );

        let mut ids = Vec::new(&env);
        ids.push_back(swap_ids.get(0).unwrap());
        ids.push_back(swap_ids.get(1).unwrap());

        // Batch deposit
        client.batch_escrow_deposit(&ids, &buyer);

        // Both should be Accepted
        assert_eq!(
            client.get_swap(&ids.get(0).unwrap()).unwrap().status,
            SwapStatus::Accepted
        );
        assert_eq!(
            client.get_swap(&ids.get(1).unwrap()).unwrap().status,
            SwapStatus::Accepted
        );
    }

    #[test]
    fn test_batch_escrow_deposit_single() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, _, _) = commit_ip(&env, &registry_id, &seller, 0x70);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);
        let mut prices = Vec::new(&env);
        prices.push_back(1000i128);
        let timeout = env.ledger().timestamp() + 3600;
        let mut timeouts = Vec::new(&env);
        timeouts.push_back(timeout);

        let swap_ids =
            client.batch_initiate_escrow(&token_id, &ip_ids, &seller, &prices, &buyer, &timeouts);

        let mut ids = Vec::new(&env);
        ids.push_back(swap_ids.get(0).unwrap());

        client.batch_escrow_deposit(&ids, &buyer);

        assert_eq!(
            client.get_swap(&ids.get(0).unwrap()).unwrap().status,
            SwapStatus::Accepted
        );
    }

    #[test]
    #[should_panic]
    fn test_batch_escrow_deposit_wrong_buyer_panics() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let other = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, _, _) = commit_ip(&env, &registry_id, &seller, 0x80);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);
        let mut prices = Vec::new(&env);
        prices.push_back(500i128);
        let timeout = env.ledger().timestamp() + 3600;
        let mut timeouts = Vec::new(&env);
        timeouts.push_back(timeout);

        let swap_ids =
            client.batch_initiate_escrow(&token_id, &ip_ids, &seller, &prices, &buyer, &timeouts);

        let mut ids = Vec::new(&env);
        ids.push_back(swap_ids.get(0).unwrap());

        // Wrong buyer — must panic
        client.batch_escrow_deposit(&ids, &other);
    }
}
