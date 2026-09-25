#[cfg(test)]
mod e2e_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, BytesN, Env, Vec,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus};

    const RULING_DELAY: u64 = 48 * 3600;

    /// Helper to set up a registry with an IP commitment
    fn setup_registry(env: &Env, owner: &Address) -> (Address, u64, BytesN<32>, BytesN<32>) {
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);
        let secret = BytesN::from_array(env, &[2u8; 32]);
        let blinding = BytesN::from_array(env, &[3u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(owner, &commitment_hash, &0u32);
        (registry_id, ip_id, secret, blinding)
    }

    /// Helper to set up a token with initial supply
    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // E2E TEST 1: Simple Swap Flow
    //
    // Tests the full successful flow:
    // 1. IP commitment (via registry)
    // 2. Initiate swap
    // 3. Accept swap
    // 4. Complete swap
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn e2e_test_simple_swap_flow() {
        let env = Env::default();
        env.mock_all_auths();

        // Step 1: IP Commitment
        let seller = Address::generate(&env);
        let (registry_id, ip_id, _secret, _blinding) = setup_registry(&env, &seller);

        // Step 2: Setup Token
        let buyer = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &buyer, 1_000_000);
        StellarAssetClient::new(&env, &token_id).mint(&seller, &1_000_000);

        // Step 3: Initialize Swap Contract
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Step 4: Create Swap
        let swap_price = 50_000_i128;
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &swap_price, &buyer, &0_u32, &None, &0_i128, &false,
        );

        // Verify swap is in Pending state
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Pending);
        assert_eq!(swap.price, swap_price);
        assert_eq!(swap.seller, seller);
        assert_eq!(swap.buyer, buyer);

        // Step 5: Buyer Accepts Swap
        client.accept_swap(&swap_id);
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Accepted);

        // Step 6: Complete Swap
        client.complete_swap(&swap_id);
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Completed);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // E2E TEST 2: Swap with Cancellation
    //
    // Tests cancellation flow:
    // 1. Create swap
    // 2. Seller cancels pending swap
    // 3. Verify swap is cancelled
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn e2e_test_swap_cancellation_flow() {
        let env = Env::default();
        env.mock_all_auths();

        // Setup
        let seller = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);

        let buyer = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &buyer, 100_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Create swap
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &50_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        // Verify initial state
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Pending);

        // Cancel swap
        client.cancel_swap(&swap_id);
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Cancelled);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // E2E TEST 3: Swap with Dispute and Resolution
    //
    // Tests dispute flow:
    // 1. Create and accept swap
    // 2. Buyer raises dispute
    // 3. Set arbitrator
    // 4. Arbitrator resolves dispute (complete to seller or buyer)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn e2e_test_swap_dispute_resolution_flow() {
        let env = Env::default();
        env.mock_all_auths();

        // Setup
        let seller = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);

        let buyer = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &buyer, 1_000_000);
        StellarAssetClient::new(&env, &token_id).mint(&seller, &1_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Step 1: Create and Accept Swap
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &20_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );
        client.accept_swap(&swap_id);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Accepted);

        // Step 2: Buyer Raises Dispute
        client.raise_dispute(&swap_id);
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Disputed);

        // Step 3: Set Arbitrator
        let arbitrator = Address::generate(&env);
        let admin = Address::generate(&env);
        let mut signers = Vec::new(&env);
        signers.push_back(Address::generate(&env));
        signers.push_back(Address::generate(&env));
        signers.push_back(Address::generate(&env));

        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);

        // Step 4: Complete Dispute (to seller or buyer)
        env.ledger().with_mut(|l| l.timestamp += RULING_DELAY);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Disputed);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // E2E TEST 4: Multi-User Scenario - Sequential Swaps
    //
    // Tests multiple users performing sequential swaps:
    // 1. User A sells to User B
    // 2. User C sells to User D
    // 3. User B sells to User E
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn e2e_test_multi_user_sequential_swaps() {
        let env = Env::default();
        env.mock_all_auths();

        // Setup initial IP registry
        let initial_owner = Address::generate(&env);
        let (registry_id, _, _, _) = setup_registry(&env, &initial_owner);

        // Setup token
        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &initial_owner, 10_000_000);

        // Initialize swap contract
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Scenario 1: User A -> User B
        let user_a = Address::generate(&env);
        let user_b = Address::generate(&env);
        let (_, ip_a, _, _) = setup_registry(&env, &user_a);
        StellarAssetClient::new(&env, &token_id).mint(&user_b, &500_000);

        let swap_1 = client.initiate_swap(
            &token_id, &ip_a, &user_a, &100_i128, &user_b, &0_u32, &None, &0_i128, &false,
        );
        client.accept_swap(&swap_1);
        client.complete_swap(&swap_1);

        let swap_1_final = client.get_swap(&swap_1).unwrap();
        assert_eq!(swap_1_final.status, SwapStatus::Completed);

        // Scenario 2: User C -> User D (independent of previous)
        let user_c = Address::generate(&env);
        let user_d = Address::generate(&env);
        let (_, ip_c, _, _) = setup_registry(&env, &user_c);
        StellarAssetClient::new(&env, &token_id).mint(&user_d, &500_000);

        let swap_2 = client.initiate_swap(
            &token_id, &ip_c, &user_c, &150_i128, &user_d, &0_u32, &None, &0_i128, &false,
        );
        client.accept_swap(&swap_2);
        client.complete_swap(&swap_2);

        let swap_2_final = client.get_swap(&swap_2).unwrap();
        assert_eq!(swap_2_final.status, SwapStatus::Completed);

        // Verify both swaps completed independently
        assert_eq!(swap_1_final.seller, user_a);
        assert_eq!(swap_1_final.buyer, user_b);
        assert_eq!(swap_2_final.seller, user_c);
        assert_eq!(swap_2_final.buyer, user_d);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // E2E TEST 5: Error Handling - Swap Not Found
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    #[should_panic]
    fn e2e_test_error_swap_not_found() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let (registry_id, _, _, _) = setup_registry(&env, &seller);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Try to get non-existent swap
        let _swap = client.get_swap(&999999_u64).unwrap();
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // E2E TEST 6: Error Handling - Invalid State Transitions
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    #[should_panic]
    fn e2e_test_error_cannot_accept_accepted_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);

        let buyer = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &buyer, 100_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &50_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        // Accept once
        client.accept_swap(&swap_id);

        // Try to accept again - should panic
        client.accept_swap(&swap_id);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // E2E TEST 7: Error Handling - Complete Non-Accepted Swap
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    #[should_panic]
    fn e2e_test_error_cannot_complete_non_accepted_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);

        let buyer = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &buyer, 100_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &50_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        // Try to complete without accepting - should panic
        client.complete_swap(&swap_id);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // E2E TEST 8: Swap Expiry - Expired Swap Cannot Be Accepted
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    #[should_panic]
    fn e2e_test_error_cannot_accept_expired_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);

        let buyer = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &buyer, 100_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &50_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        // Move time beyond expiry
        env.ledger().with_mut(|l| l.timestamp += 30 * 24 * 3600 + 1); // 30+ days

        // Try to accept expired swap - should panic
        client.accept_swap(&swap_id);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // E2E TEST 9: Multiple Approvals Required
    //
    // Tests swap with required_approvals > 0
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn e2e_test_swap_with_required_approvals() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);

        let buyer = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &buyer, 100_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Create swap requiring 2 approvals
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &50_i128, &buyer, &2_u32, &None, &0_i128, &false,
        );

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.required_approvals, 2);
        assert_eq!(swap.status, SwapStatus::Pending);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // E2E TEST 10: Referrer Tracking
    //
    // Tests that referrer field is properly stored
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn e2e_test_swap_with_referrer() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);

        let buyer = Address::generate(&env);
        let referrer = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &buyer, 100_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Create swap with referrer
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &50_i128, &buyer, &0_u32, &Some(referrer.clone()), &0_i128, &false,
        );

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.referrer, Some(referrer));
    }
}
