#[cfg(test)]
mod arbitration_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus};

    fn setup_registry(env: &Env, owner: &Address) -> (Address, u64, BytesN<32>, BytesN<32>) {
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);
        let secret = BytesN::from_array(env, &[2u8; 32]);
        let blinding = BytesN::from_array(env, &[3u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(owner, &commitment_hash);
        (registry_id, ip_id, secret, blinding)
    }

    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    fn setup_disputed_swap(env: &Env) -> (AtomicSwapClient, u64, Address, Address) {
        let seller = Address::generate(env);
        let buyer = Address::generate(env);
        let admin = Address::generate(env);
        let (registry_id, ip_id, _, _) = setup_registry(env, &seller);
        let token_id = setup_token(env, &admin, &buyer, 1000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32);
        client.accept_swap(&swap_id);
        client.raise_dispute(&swap_id);

        (client, swap_id, seller, buyer)
    }

    // ── #314: set_arbitrator ──────────────────────────────────────────────────

    #[test]
    fn test_set_arbitrator_on_disputed_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, _) = setup_disputed_swap(&env);
        let arbitrator = Address::generate(&env);
        let admin = Address::generate(&env);

        // Admin sets arbitrator
        client.set_arbitrator(&swap_id, &admin, &arbitrator);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.arbitrator, Some(arbitrator));
    }

    #[test]
    #[should_panic]
    fn test_set_arbitrator_twice_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, _) = setup_disputed_swap(&env);
        let arbitrator = Address::generate(&env);
        let admin = Address::generate(&env);

        client.set_arbitrator(&swap_id, &admin, &arbitrator);
        // Second call should panic with ArbitratorAlreadySet
        client.set_arbitrator(&swap_id, &admin, &arbitrator);
    }

    #[test]
    #[should_panic]
    fn test_set_arbitrator_on_non_disputed_swap_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin_token = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin_token, &buyer, 1000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32);
        // Swap is Pending, not Disputed — should panic
        let admin = Address::generate(&env);
        let arbitrator = Address::generate(&env);
        client.set_arbitrator(&swap_id, &admin, &arbitrator);
    }

    // ── #314: arbitrate_dispute ───────────────────────────────────────────────

    #[test]
    fn test_arbitrate_dispute_refunds_buyer() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, buyer) = setup_disputed_swap(&env);
        let arbitrator = Address::generate(&env);
        let admin = Address::generate(&env);

        client.set_arbitrator(&swap_id, &admin, &arbitrator);
        client.arbitrate_dispute(&swap_id, &arbitrator, &true);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Cancelled);
    }

    #[test]
    fn test_arbitrate_dispute_completes_to_seller() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, _) = setup_disputed_swap(&env);
        let arbitrator = Address::generate(&env);
        let admin = Address::generate(&env);

        client.set_arbitrator(&swap_id, &admin, &arbitrator);
        client.arbitrate_dispute(&swap_id, &arbitrator, &false);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Completed);
    }

    #[test]
    #[should_panic]
    fn test_wrong_arbitrator_cannot_arbitrate() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, _) = setup_disputed_swap(&env);
        let arbitrator = Address::generate(&env);
        let impostor = Address::generate(&env);
        let admin = Address::generate(&env);

        client.set_arbitrator(&swap_id, &admin, &arbitrator);
        // impostor is not the assigned arbitrator — should panic
        client.arbitrate_dispute(&swap_id, &impostor, &true);
    }

    #[test]
    #[should_panic]
    fn test_arbitrate_without_arbitrator_set_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, _) = setup_disputed_swap(&env);
        let anyone = Address::generate(&env);
        // No arbitrator set — should panic with NoArbitratorSet
        client.arbitrate_dispute(&swap_id, &anyone, &true);
    }

    // ── #313: submit_dispute_evidence ────────────────────────────────────────

    #[test]
    fn test_buyer_can_submit_evidence() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, buyer) = setup_disputed_swap(&env);
        let hash = BytesN::from_array(&env, &[0xabu8; 32]);

        client.submit_dispute_evidence(&swap_id, &buyer, &hash);

        let evidence = client.get_dispute_evidence(&swap_id);
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence.get(0).unwrap(), hash);
    }

    #[test]
    fn test_seller_can_submit_evidence() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, seller, _) = setup_disputed_swap(&env);
        let hash = BytesN::from_array(&env, &[0xbbu8; 32]);

        client.submit_dispute_evidence(&swap_id, &seller, &hash);

        let evidence = client.get_dispute_evidence(&swap_id);
        assert_eq!(evidence.len(), 1);
    }

    #[test]
    fn test_multiple_evidence_submissions_accumulate() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, seller, buyer) = setup_disputed_swap(&env);
        let hash1 = BytesN::from_array(&env, &[0x01u8; 32]);
        let hash2 = BytesN::from_array(&env, &[0x02u8; 32]);

        client.submit_dispute_evidence(&swap_id, &buyer, &hash1);
        client.submit_dispute_evidence(&swap_id, &seller, &hash2);

        let evidence = client.get_dispute_evidence(&swap_id);
        assert_eq!(evidence.len(), 2);
    }

    #[test]
    #[should_panic]
    fn test_third_party_cannot_submit_evidence() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, _) = setup_disputed_swap(&env);
        let outsider = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[0xffu8; 32]);

        // outsider is neither buyer nor seller — should panic
        client.submit_dispute_evidence(&swap_id, &outsider, &hash);
    }

    #[test]
    fn test_get_dispute_evidence_empty_for_new_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, _) = setup_disputed_swap(&env);
        let evidence = client.get_dispute_evidence(&swap_id);
        assert_eq!(evidence.len(), 0);
    }

    // ── #312: tiered pricing ──────────────────────────────────────────────────

    #[test]
    fn test_accept_swap_with_quantity_applies_tier() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Initiate with flat price 500
        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32);

        // Accept with quantity=1 (no tiers set, uses flat price)
        client.accept_swap_with_quantity(&swap_id, &1_u32);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Accepted);
        assert_eq!(swap.price, 500);
    }

    #[test]
    fn test_accept_swap_flat_price_when_no_tiers() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &1000_i128, &buyer, &0_u32);
        client.accept_swap_with_quantity(&swap_id, &5_u32);

        let swap = client.get_swap(&swap_id).unwrap();
        // No tiers: price stays at flat 1000
        assert_eq!(swap.price, 1000);
        assert_eq!(swap.status, SwapStatus::Accepted);
    }
}
