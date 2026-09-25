#[cfg(test)]
mod mutation_testing_coverage {
    /// This module contains tests specifically designed to catch mutations
    /// in critical contract functions. These tests are designed to maximize
    /// mutation testing effectiveness.
    ///
    /// MUTATION TESTING STRATEGY:
    ///
    /// Mutation testing works by introducing small changes to the code
    /// (mutations) and verifying that tests still fail. If a test passes
    /// despite a mutation, that mutation "survived" and the test is weak.
    ///
    /// This module focuses on tests that would catch common mutations:
    /// - Boundary condition changes (< vs <=, > vs >=)
    /// - Boolean inversions (true/false)
    /// - Arithmetic operator changes (+/-, *//)
    /// - Return value modifications
    /// - Condition removals
    /// - State variable modifications

    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, BytesN, Env, Vec,
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
        let ip_id = registry.commit_ip(owner, &commitment_hash, &0u32);
        (registry_id, ip_id, secret, blinding)
    }

    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // MUTATION KILLER: Swap Status Equality Tests
    //
    // These tests will kill any mutation that:
    // - Changes status values
    // - Changes status comparison operators
    // - Skips status updates
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn mutation_killer_swap_status_pending() {
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

        // Catches mutations that change initial status or skip status assignment
        let swap = client.get_swap(&swap_id).unwrap();
        assert!(swap.status == SwapStatus::Pending);
    }

    #[test]
    fn mutation_killer_swap_status_accepted() {
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

        // Catches mutations that skip accept_swap logic
        client.accept_swap(&swap_id);
        let swap = client.get_swap(&swap_id).unwrap();
        assert!(swap.status == SwapStatus::Accepted);
    }

    #[test]
    fn mutation_killer_swap_status_completed() {
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

        client.accept_swap(&swap_id);
        // Catches mutations that skip complete_swap logic
        client.complete_swap(&swap_id);
        let swap = client.get_swap(&swap_id).unwrap();
        assert!(swap.status == SwapStatus::Completed);
    }

    #[test]
    fn mutation_killer_swap_status_cancelled() {
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

        // Catches mutations that skip cancel_swap logic
        client.cancel_swap(&swap_id);
        let swap = client.get_swap(&swap_id).unwrap();
        assert!(swap.status == SwapStatus::Cancelled);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // MUTATION KILLER: Price Field Integrity
    //
    // Tests for mutations in price-related logic
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn mutation_killer_price_field_stored_correctly() {
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

        let test_price = 12345_i128;
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &test_price, &buyer, &0_u32, &None, &0_i128, &false,
        );

        // Catches mutations that modify price values or skip storage
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.price, test_price);
    }

    #[test]
    fn mutation_killer_price_boundary_values() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);

        let buyer = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &buyer, 1_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Test with minimal price
        let swap_id_1 = client.initiate_swap(
            &token_id, &ip_id, &seller, &1_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        let swap_1 = client.get_swap(&swap_id_1).unwrap();
        assert_eq!(swap_1.price, 1);

        // Test with large price
        let swap_id_2 = client.initiate_swap(
            &token_id, &ip_id, &seller, &999_999_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        let swap_2 = client.get_swap(&swap_id_2).unwrap();
        assert_eq!(swap_2.price, 999_999);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // MUTATION KILLER: Participant Identity Preservation
    //
    // Tests that catch mutations in seller/buyer assignment or comparison
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn mutation_killer_seller_preserved() {
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

        // Catches mutations that mix up seller/buyer or modify assignments
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.seller, seller);
    }

    #[test]
    fn mutation_killer_buyer_preserved() {
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

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.buyer, buyer);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // MUTATION KILLER: State Transition Logic
    //
    // Tests that catch mutations in conditional logic for state changes
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    #[should_panic]
    fn mutation_killer_cannot_accept_when_not_pending() {
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

        client.accept_swap(&swap_id);
        // Catches mutations that remove status checks
        client.accept_swap(&swap_id);
    }

    #[test]
    #[should_panic]
    fn mutation_killer_cannot_complete_when_not_accepted() {
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

        // Catches mutations that remove status checks in complete_swap
        client.complete_swap(&swap_id);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // MUTATION KILLER: Required Approvals Tracking
    //
    // Tests that catch mutations in approval count logic
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn mutation_killer_required_approvals_zero() {
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

        // Catches mutations that modify required_approvals
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.required_approvals, 0);
    }

    #[test]
    fn mutation_killer_required_approvals_nonzero() {
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
            &token_id, &ip_id, &seller, &50_i128, &buyer, &5_u32, &None, &0_i128, &false,
        );

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.required_approvals, 5);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // MUTATION KILLER: Token Field Integrity
    //
    // Tests that catch mutations in token storage/retrieval
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn mutation_killer_token_field_preserved() {
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

        // Catches mutations that modify or skip token field
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.token, token_id);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // MUTATION KILLER: IP ID Field Integrity
    //
    // Tests that catch mutations in ip_id storage
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn mutation_killer_ip_id_preserved() {
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

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.ip_id, ip_id);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // MUTATION KILLER: Insurance Flag Logic
    //
    // Tests that catch mutations in boolean flag handling
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn mutation_killer_insurance_enabled_false() {
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

        // Catches mutations that invert boolean values
        let swap = client.get_swap(&swap_id).unwrap();
        assert!(!swap.insurance_enabled);
    }

    #[test]
    fn mutation_killer_insurance_enabled_true() {
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
            &token_id, &ip_id, &seller, &50_i128, &buyer, &0_u32, &None, &0_i128, &true,
        );

        let swap = client.get_swap(&swap_id).unwrap();
        assert!(swap.insurance_enabled);
    }
}
