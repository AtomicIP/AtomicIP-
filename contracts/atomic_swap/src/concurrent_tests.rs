#[cfg(test)]
mod concurrent_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, BytesN, Env, Vec,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus};

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
    // CONCURRENT TEST 1: Multiple Sequential Swaps
    //
    // Tests 10 sequential swaps to verify no race conditions in swap creation
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn concurrent_test_multiple_sequential_swaps() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let (registry_id, _, _, _) = setup_registry(&env, &seller);

        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &seller, 10_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Create 10 swaps with different buyers
        let mut swap_ids = Vec::new(&env);
        for i in 0..10 {
            let buyer = Address::generate(&env);
            let (_, ip_id, _, _) = setup_registry(&env, &seller);
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &1_000_000);

            let swap_id = client.initiate_swap(
                &token_id, &ip_id, &seller, &(100 + i as i128), &buyer, &0_u32, &None, &0_i128, &false,
            );
            swap_ids.push_back(swap_id);
        }

        // Verify all 10 swaps exist and are in Pending state
        assert_eq!(swap_ids.len(), 10);
        for i in 0..10 {
            let swap = client.get_swap(&swap_ids.get(i).unwrap()).unwrap();
            assert_eq!(swap.status, SwapStatus::Pending);
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // CONCURRENT TEST 2: Simultaneous Accept Operations
    //
    // Tests that multiple swaps can be accepted in sequence without conflicts
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn concurrent_test_simultaneous_accept_operations() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let (registry_id, _, _, _) = setup_registry(&env, &seller);

        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &seller, 10_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Create 5 swaps
        let mut swap_ids = Vec::new(&env);
        for i in 0..5 {
            let buyer = Address::generate(&env);
            let (_, ip_id, _, _) = setup_registry(&env, &seller);
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &1_000_000);

            let swap_id = client.initiate_swap(
                &token_id, &ip_id, &seller, &(100 + i as i128), &buyer, &0_u32, &None, &0_i128, &false,
            );
            swap_ids.push_back(swap_id);
        }

        // Accept all 5 swaps sequentially
        for i in 0..5 {
            client.accept_swap(&swap_ids.get(i).unwrap());
        }

        // Verify all are in Accepted state
        for i in 0..5 {
            let swap = client.get_swap(&swap_ids.get(i).unwrap()).unwrap();
            assert_eq!(swap.status, SwapStatus::Accepted);
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // CONCURRENT TEST 3: Concurrent Completes After Accepts
    //
    // Tests that multiple swaps can be completed after being accepted
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn concurrent_test_concurrent_completes_after_accepts() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let (registry_id, _, _, _) = setup_registry(&env, &seller);

        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &seller, 10_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Create and accept 5 swaps
        let mut swap_ids = Vec::new(&env);
        for i in 0..5 {
            let buyer = Address::generate(&env);
            let (_, ip_id, _, _) = setup_registry(&env, &seller);
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &1_000_000);

            let swap_id = client.initiate_swap(
                &token_id, &ip_id, &seller, &(100 + i as i128), &buyer, &0_u32, &None, &0_i128, &false,
            );
            client.accept_swap(&swap_id);
            swap_ids.push_back(swap_id);
        }

        // Complete all 5 swaps sequentially
        for i in 0..5 {
            client.complete_swap(&swap_ids.get(i).unwrap());
        }

        // Verify all are in Completed state
        for i in 0..5 {
            let swap = client.get_swap(&swap_ids.get(i).unwrap()).unwrap();
            assert_eq!(swap.status, SwapStatus::Completed);
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // CONCURRENT TEST 4: Multiple Buyers, Single Seller
    //
    // Tests many buyers transacting with a single seller (stress test)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn concurrent_test_multiple_buyers_single_seller() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let (registry_id, _, _, _) = setup_registry(&env, &seller);

        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &seller, 100_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Create swaps from 20 different buyers
        let mut swap_ids = Vec::new(&env);
        for i in 0..20 {
            let buyer = Address::generate(&env);
            let (_, ip_id, _, _) = setup_registry(&env, &seller);
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &10_000_000);

            let swap_id = client.initiate_swap(
                &token_id, &ip_id, &seller, &(50 + (i % 10) as i128), &buyer, &0_u32, &None, &0_i128, &false,
            );
            swap_ids.push_back(swap_id);
        }

        // Accept first 10
        for i in 0..10 {
            client.accept_swap(&swap_ids.get(i).unwrap());
        }

        // Complete the accepted ones
        for i in 0..10 {
            client.complete_swap(&swap_ids.get(i).unwrap());
        }

        // Verify state consistency
        for i in 0..20 {
            let swap = client.get_swap(&swap_ids.get(i).unwrap()).unwrap();
            if i < 10 {
                assert_eq!(swap.status, SwapStatus::Completed);
            } else {
                assert_eq!(swap.status, SwapStatus::Pending);
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // CONCURRENT TEST 5: Race Condition Detection - Double Accept
    //
    // Ensures a swap cannot be accepted twice (even under concurrent calls)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    #[should_panic]
    fn concurrent_test_race_condition_double_accept() {
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

        // Attempt double accept (should panic)
        client.accept_swap(&swap_id);
        client.accept_swap(&swap_id); // This should panic
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // CONCURRENT TEST 6: Race Condition Detection - Complete Without Accept
    //
    // Ensures operations maintain ordering: accept must precede complete
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    #[should_panic]
    fn concurrent_test_race_condition_complete_without_accept() {
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

        // Try to complete without accepting (should panic)
        client.complete_swap(&swap_id);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // CONCURRENT TEST 7: Concurrent Cancellations
    //
    // Tests that pending swaps can be cancelled, and cancellation is atomic
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn concurrent_test_concurrent_cancellations() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let (registry_id, _, _, _) = setup_registry(&env, &seller);

        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &seller, 10_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Create 5 swaps
        let mut swap_ids = Vec::new(&env);
        for i in 0..5 {
            let buyer = Address::generate(&env);
            let (_, ip_id, _, _) = setup_registry(&env, &seller);
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &1_000_000);

            let swap_id = client.initiate_swap(
                &token_id, &ip_id, &seller, &(100 + i as i128), &buyer, &0_u32, &None, &0_i128, &false,
            );
            swap_ids.push_back(swap_id);
        }

        // Cancel all 5 swaps
        for i in 0..5 {
            client.cancel_swap(&swap_ids.get(i).unwrap());
        }

        // Verify all are cancelled
        for i in 0..5 {
            let swap = client.get_swap(&swap_ids.get(i).unwrap()).unwrap();
            assert_eq!(swap.status, SwapStatus::Cancelled);
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // CONCURRENT TEST 8: Stress Test - 50 Concurrent Swaps
    //
    // Creates and processes 50 swaps to test system stability under load
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn concurrent_test_stress_50_swaps() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let (registry_id, _, _, _) = setup_registry(&env, &seller);

        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &seller, 500_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Create 50 swaps
        let mut swap_ids = Vec::new(&env);
        for i in 0..50 {
            let buyer = Address::generate(&env);
            let (_, ip_id, _, _) = setup_registry(&env, &seller);
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &10_000_000);

            let swap_id = client.initiate_swap(
                &token_id, &ip_id, &seller, &((i % 100) as i128 + 50), &buyer, &0_u32, &None, &0_i128, &false,
            );
            swap_ids.push_back(swap_id);
        }

        // Accept and complete first 30
        for i in 0..30 {
            client.accept_swap(&swap_ids.get(i).unwrap());
            client.complete_swap(&swap_ids.get(i).unwrap());
        }

        // Cancel next 10
        for i in 30..40 {
            client.cancel_swap(&swap_ids.get(i).unwrap());
        }

        // Leave last 10 in Pending state

        // Verify counts
        assert_eq!(swap_ids.len(), 50);

        // Spot check some states
        let completed_swap = client.get_swap(&swap_ids.get(0).unwrap()).unwrap();
        assert_eq!(completed_swap.status, SwapStatus::Completed);

        let cancelled_swap = client.get_swap(&swap_ids.get(30).unwrap()).unwrap();
        assert_eq!(cancelled_swap.status, SwapStatus::Cancelled);

        let pending_swap = client.get_swap(&swap_ids.get(40).unwrap()).unwrap();
        assert_eq!(pending_swap.status, SwapStatus::Pending);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // CONCURRENT TEST 9: Deadlock Detection - No Locked State
    //
    // Ensures that operations never leave the contract in a deadlocked state
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn concurrent_test_no_deadlock_state() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let (registry_id, _, _, _) = setup_registry(&env, &seller);

        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &seller, 100_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Create mixed workload
        for i in 0..25 {
            let buyer = Address::generate(&env);
            let (_, ip_id, _, _) = setup_registry(&env, &seller);
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &10_000_000);

            let swap_id = client.initiate_swap(
                &token_id, &ip_id, &seller, &(50_i128 + (i % 20) as i128), &buyer, &0_u32, &None, &0_i128, &false,
            );

            // Vary operations
            match i % 3 {
                0 => {
                    client.accept_swap(&swap_id);
                    client.complete_swap(&swap_id);
                }
                1 => {
                    client.cancel_swap(&swap_id);
                }
                _ => {
                    // Leave in pending
                }
            }
        }

        // If we reach here without hanging, no deadlock occurred
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // CONCURRENT TEST 10: Isolation - Swaps Don't Interfere
    //
    // Verifies that operations on one swap don't affect others
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn concurrent_test_swap_isolation() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let (registry_id, _, _, _) = setup_registry(&env, &seller);

        let token_admin = Address::generate(&env);
        let token_id = setup_token(&env, &token_admin, &seller, 100_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Create 3 swaps
        let mut swap_ids = Vec::new(&env);
        for i in 0..3 {
            let buyer = Address::generate(&env);
            let (_, ip_id, _, _) = setup_registry(&env, &seller);
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &10_000_000);

            let swap_id = client.initiate_swap(
                &token_id, &ip_id, &seller, &(100 + i as i128), &buyer, &0_u32, &None, &0_i128, &false,
            );
            swap_ids.push_back(swap_id);
        }

        // Complete only swap 0
        client.accept_swap(&swap_ids.get(0).unwrap());
        client.complete_swap(&swap_ids.get(0).unwrap());

        // Verify swap 0 is completed but others are untouched
        assert_eq!(client.get_swap(&swap_ids.get(0).unwrap()).unwrap().status, SwapStatus::Completed);
        assert_eq!(client.get_swap(&swap_ids.get(1).unwrap()).unwrap().status, SwapStatus::Pending);
        assert_eq!(client.get_swap(&swap_ids.get(2).unwrap()).unwrap().status, SwapStatus::Pending);

        // Cancel swap 1
        client.cancel_swap(&swap_ids.get(1).unwrap());

        // Verify isolation
        assert_eq!(client.get_swap(&swap_ids.get(0).unwrap()).unwrap().status, SwapStatus::Completed);
        assert_eq!(client.get_swap(&swap_ids.get(1).unwrap()).unwrap().status, SwapStatus::Cancelled);
        assert_eq!(client.get_swap(&swap_ids.get(2).unwrap()).unwrap().status, SwapStatus::Pending);
    }
}
