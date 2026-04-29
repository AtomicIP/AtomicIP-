#[cfg(test)]
mod tests {
    use atomic_swap::{AtomicSwap, AtomicSwapClient, SwapCondition, SwapStatus};
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        vec, Address, BytesN, Env,
    };

    fn setup_registry(env: &Env, owner: &Address) -> (Address, u64) {
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);
        let secret = BytesN::from_array(env, &[2u8; 32]);
        let blinding = BytesN::from_array(env, &[3u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(owner, &commitment_hash);
        (registry_id, ip_id)
    }

    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    fn setup_swap(env: &Env) -> (AtomicSwapClient, u64, Address, Address) {
        let seller = Address::generate(env);
        let buyer = Address::generate(env);
        let admin = Address::generate(env);
        let (registry_id, ip_id) = setup_registry(env, &seller);
        let token_id = setup_token(env, &admin, &buyer, 10_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &1000_i128, &buyer, &0_u32, &None, &false);
        (client, swap_id, seller, buyer)
    }

    // ── renegotiation ─────────────────────────────────────────────────────────

    #[test]
    fn test_propose_renegotiation() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        // Seller proposes a new price
        client.renegotiate_swap(&swap_id, &800_i128);

        // Swap price unchanged until buyer accepts
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.price, 1000);
        assert_eq!(swap.status, SwapStatus::Pending);
    }

    #[test]
    fn test_accept_renegotiation() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        client.renegotiate_swap(&swap_id, &750_i128);
        client.accept_renegotiation(&swap_id);

        // Price updated to the negotiated value
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.price, 750);
        assert_eq!(swap.status, SwapStatus::Pending);
    }

    #[test]
    fn test_renegotiation_then_accept_swap_uses_new_price() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        client.renegotiate_swap(&swap_id, &600_i128);
        client.accept_renegotiation(&swap_id);
        // Buyer now accepts the swap at the renegotiated price
        client.accept_swap(&swap_id);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Accepted);
        assert_eq!(swap.price, 600);
    }

    #[test]
    #[should_panic]
    fn test_accept_renegotiation_without_offer_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        // No offer proposed — should panic
        client.accept_renegotiation(&swap_id);
    }

    #[test]
    #[should_panic]
    fn test_renegotiate_zero_price_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        client.renegotiate_swap(&swap_id, &0_i128);
    }

    #[test]
    fn test_renegotiate_overwrites_previous_offer() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        client.renegotiate_swap(&swap_id, &900_i128);
        // Seller changes their mind
        client.renegotiate_swap(&swap_id, &850_i128);
        client.accept_renegotiation(&swap_id);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.price, 850);
    }

    #[test]
    fn test_accept_swap_with_insurance() {
        // placeholder — insurance feature not yet implemented
    }

    #[test]
    fn test_initiate_swap_with_escrow() {
        // placeholder — escrow feature not yet implemented
    }

    #[test]
    fn test_escrow_release_funds() {
        // placeholder — escrow feature not yet implemented
    }

    #[test]
    fn test_accept_swap_conditional() {
        // placeholder — conditional acceptance not yet implemented
    }
}

    // ── conditional acceptance ────────────────────────────────────────────────

    #[test]
    fn test_accept_swap_conditional_key_valid_passes() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        // KeyValid always passes at accept time
        let conditions = vec![&env, SwapCondition::KeyValid];
        client.accept_swap_conditional(&swap_id, &conditions);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Accepted);
    }

    #[test]
    fn test_accept_swap_conditional_price_below_passes() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        // price=1000, threshold=2000 → passes
        let conditions = vec![&env, SwapCondition::PriceBelow(2000)];
        client.accept_swap_conditional(&swap_id, &conditions);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Accepted);
    }

    #[test]
    #[should_panic]
    fn test_accept_swap_conditional_price_below_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        // price=1000, threshold=500 → fails
        let conditions = vec![&env, SwapCondition::PriceBelow(500)];
        client.accept_swap_conditional(&swap_id, &conditions);
    }

    #[test]
    fn test_accept_swap_conditional_time_after_passes() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        // Advance ledger past the threshold
        env.ledger().with_mut(|l| l.timestamp = 1000);
        let conditions = vec![&env, SwapCondition::TimeAfter(500)];
        client.accept_swap_conditional(&swap_id, &conditions);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Accepted);
    }

    #[test]
    #[should_panic]
    fn test_accept_swap_conditional_time_after_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        // Ledger timestamp is 0, threshold is in the future
        let conditions = vec![&env, SwapCondition::TimeAfter(9999)];
        client.accept_swap_conditional(&swap_id, &conditions);
    }

    #[test]
    fn test_accept_swap_conditional_multiple_conditions_all_pass() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        env.ledger().with_mut(|l| l.timestamp = 100);
        let conditions = vec![
            &env,
            SwapCondition::KeyValid,
            SwapCondition::PriceBelow(5000),
            SwapCondition::TimeAfter(50),
        ];
        client.accept_swap_conditional(&swap_id, &conditions);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Accepted);
        assert_eq!(swap.conditions.len(), 3);
    }

    #[test]
    fn test_accept_swap_unconditional_ignores_empty_conditions() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        // Plain accept_swap still works with no conditions
        client.accept_swap(&swap_id);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Accepted);
        assert_eq!(swap.conditions.len(), 0);
    }

    #[test]
    fn test_accept_swap_with_insurance() {
        // placeholder — insurance feature not yet implemented
    }

    #[test]
    fn test_initiate_swap_with_escrow() {
        // placeholder — escrow feature not yet implemented
    }

    #[test]
    fn test_escrow_release_funds() {
        // placeholder — escrow feature not yet implemented
    }

    // ── insurance ─────────────────────────────────────────────────────────────

    fn setup_insured_swap(env: &Env) -> (AtomicSwapClient, u64, Address, Address, Address, BytesN<32>, BytesN<32>) {
        let seller = Address::generate(env);
        let buyer = Address::generate(env);
        let admin = Address::generate(env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);
        let secret = BytesN::from_array(env, &[2u8; 32]);
        let blinding = BytesN::from_array(env, &[3u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &commitment_hash);

        // Mint enough for price (1000) + insurance premium (2% = 20)
        let token_id = setup_token(env, &admin, &buyer, 2000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000_i128, &buyer, &0_u32, &None, &0_i128, &None, &true,
        );
        (client, swap_id, seller, buyer, token_id, secret, blinding)
    }

    #[test]
    fn test_accept_swap_with_insurance_collects_premium() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer, token_id, _secret, _blinding) =
            setup_insured_swap(&env);

        client.accept_swap(&swap_id);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Accepted);
        assert_eq!(swap.insurance_enabled, true);
        // premium = 2% of 1000 = 20
        assert_eq!(swap.insurance_premium, 20);
    }

    #[test]
    fn test_claim_insurance_after_invalid_key() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, seller, buyer, token_id, _secret, _blinding) =
            setup_insured_swap(&env);

        client.accept_swap(&swap_id);

        // Seller reveals a wrong key — this panics but sets InsuranceClaimable first
        let wrong_secret = BytesN::from_array(&env, &[9u8; 32]);
        let wrong_blinding = BytesN::from_array(&env, &[9u8; 32]);
        let _ = client.try_reveal_key(&swap_id, &seller, &wrong_secret, &wrong_blinding);

        // Buyer claims insurance
        let token = soroban_sdk::token::Client::new(&env, &token_id);
        let balance_before = token.balance(&buyer);
        client.claim_insurance(&swap_id);
        let balance_after = token.balance(&buyer);

        // Buyer gets back the swap price (1000) from the pool
        assert_eq!(balance_after - balance_before, 1000);
    }

    #[test]
    #[should_panic]
    fn test_claim_insurance_without_invalid_key_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer, _token_id, _secret, _blinding) =
            setup_insured_swap(&env);

        client.accept_swap(&swap_id);
        // No invalid key revealed — claim should panic
        client.claim_insurance(&swap_id);
    }

    #[test]
    #[should_panic]
    fn test_claim_insurance_not_enabled_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        // swap has no insurance
        client.accept_swap(&swap_id);
        client.claim_insurance(&swap_id);
    }

    #[test]
    #[should_panic]
    fn test_claim_insurance_twice_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, seller, _buyer, _token_id, _secret, _blinding) =
            setup_insured_swap(&env);

        client.accept_swap(&swap_id);

        let wrong_secret = BytesN::from_array(&env, &[9u8; 32]);
        let wrong_blinding = BytesN::from_array(&env, &[9u8; 32]);
        let _ = client.try_reveal_key(&swap_id, &seller, &wrong_secret, &wrong_blinding);

        client.claim_insurance(&swap_id);
        // Second claim must panic — claimable flag was cleared
        client.claim_insurance(&swap_id);
    }
}
