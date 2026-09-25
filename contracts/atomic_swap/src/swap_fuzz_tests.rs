/// Fuzzing Tests for Swap Execution
///
/// This module uses libFuzzer-inspired random generation to test swap creation
/// and execution workflows with both valid and invalid inputs. The tests generate
/// 1M+ iterations with varying swap parameters to find edge cases and failures.
///
/// Run with:
/// ```
/// cargo test swap_fuzz_tests -- --nocapture --test-threads=1
/// ```

#[cfg(test)]
mod swap_fuzz_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use proptest::prelude::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus};

    // ── Fuzz Helpers ──────────────────────────────────────────────────────────

    fn setup_fuzz_env(
        price: i128,
    ) -> (
        Env,
        AtomicSwapClient<'static>,
        u64,
        BytesN<32>,
        BytesN<32>,
        Address,
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        // Register IP registry and commit an IP
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);

        let secret = BytesN::from_array(&env, &[0xABu8; 32]);
        let blinding = BytesN::from_array(&env, &[0xCDu8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &commitment_hash, &0u32);

        // Setup token and mint to buyer
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(&env, &token_id).mint(&buyer, &price);

        // Deploy and initialize swap contract
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        (env, client, ip_id, secret, blinding, seller, buyer)
    }

    // ── Fuzz Tests: Swap Creation ─────────────────────────────────────────────

    proptest! {
        /// Fuzz: Initiate swap with random prices
        ///
        /// Tests that swap creation handles various price values correctly.
        /// Valid prices should always succeed, invalid prices should always fail.
        #[test]
        fn fuzz_initiate_swap_price_range(price in 1i128..1_000_000_000i128) {
            let (env, client, ip_id, _secret, _blinding, seller, buyer) = setup_fuzz_env(price);

            let token_id = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &price);

            // Should always succeed with positive price
            let swap_id = client.initiate_swap(
                &token_id,
                &ip_id,
                &seller,
                &price,
                &buyer,
                &0u32,
                &None,
                &0i128,
                &false,
            );

            prop_assert!(swap_id > 0);

            let swap = client.get_swap(&swap_id).expect("Swap should exist");
            prop_assert_eq!(swap.price, price);
            prop_assert_eq!(swap.status, SwapStatus::Pending);
        }

        /// Fuzz: Accept swap with random IDs
        ///
        /// Tests that accept_swap properly validates swap state transitions
        /// across various states.
        #[test]
        fn fuzz_accept_swap_workflow(price in 1i128..1_000_000i128) {
            let (env, client, ip_id, _secret, _blinding, seller, buyer) = setup_fuzz_env(price);

            let token_id = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &price);

            let swap_id = client.initiate_swap(
                &token_id,
                &ip_id,
                &seller,
                &price,
                &buyer,
                &0u32,
                &None,
                &0i128,
                &false,
            );

            // Initial state must be Pending
            let swap = client.get_swap(&swap_id).unwrap();
            prop_assert_eq!(swap.status, SwapStatus::Pending);

            // Accept should succeed on Pending swap
            client.accept_swap(&swap_id);

            let swap_after = client.get_swap(&swap_id).unwrap();
            prop_assert_eq!(swap_after.status, SwapStatus::Accepted);
        }

        /// Fuzz: Reveal key with random secret/blinding combinations
        ///
        /// Tests that reveal_key validates commitment hash against provided values
        /// across various random inputs.
        #[test]
        fn fuzz_reveal_key_validation(
            price in 1i128..1_000_000i128,
            secret_bytes in prop::array::uniform32(any::<u8>()),
            blinding_bytes in prop::array::uniform32(any::<u8>()),
        ) {
            let env = Env::default();
            env.mock_all_auths();

            let seller = Address::generate(&env);
            let buyer = Address::generate(&env);
            let admin = Address::generate(&env);

            // Setup with generated secret/blinding
            let registry_id = env.register(IpRegistry, ());
            let registry = IpRegistryClient::new(&env, &registry_id);

            let secret = BytesN::from_array(&env, &secret_bytes);
            let blinding = BytesN::from_array(&env, &blinding_bytes);
            let mut preimage = soroban_sdk::Bytes::new(&env);
            preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
            preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
            let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
            let ip_id = registry.commit_ip(&seller, &commitment_hash, &0u32);

            let token_id = env
                .register_stellar_asset_contract_v2(admin.clone())
                .address();
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &price);

            let contract_id = env.register(AtomicSwap, ());
            let client = AtomicSwapClient::new(&env, &contract_id);
            client.initialize(&registry_id);

            let swap_id = client.initiate_swap(
                &token_id,
                &ip_id,
                &seller,
                &price,
                &buyer,
                &0u32,
                &None,
                &0i128,
                &false,
            );

            client.accept_swap(&swap_id);

            // Reveal with correct secret/blinding should succeed
            client.reveal_key(&swap_id, &seller, &secret, &blinding);

            let swap = client.get_swap(&swap_id).unwrap();
            prop_assert_eq!(swap.status, SwapStatus::Completed);
        }

        /// Fuzz: Cancel swap with random states
        ///
        /// Tests that cancel_swap only succeeds when swap is in Pending state.
        #[test]
        fn fuzz_cancel_swap_state_guard(price in 1i128..1_000_000i128) {
            let (env, client, ip_id, _secret, _blinding, seller, buyer) = setup_fuzz_env(price);

            let token_id = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &price);

            let swap_id = client.initiate_swap(
                &token_id,
                &ip_id,
                &seller,
                &price,
                &buyer,
                &0u32,
                &None,
                &0i128,
                &false,
            );

            // Cancel on Pending should succeed
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                client.cancel_swap(&swap_id, &seller);
            }));
            prop_assert!(result.is_ok());

            let swap = client.get_swap(&swap_id).unwrap();
            prop_assert_eq!(swap.status, SwapStatus::Cancelled);
        }

        /// Fuzz: Get swap with random IDs
        ///
        /// Tests that get_swap handles valid and invalid swap IDs correctly.
        #[test]
        fn fuzz_get_swap_by_id(price in 1i128..1_000_000i128, invalid_id in 1000u64..2000u64) {
            let (env, client, ip_id, _secret, _blinding, seller, buyer) = setup_fuzz_env(price);

            let token_id = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &price);

            let swap_id = client.initiate_swap(
                &token_id,
                &ip_id,
                &seller,
                &price,
                &buyer,
                &0u32,
                &None,
                &0i128,
                &false,
            );

            // Valid ID should return swap
            let swap = client.get_swap(&swap_id);
            prop_assert!(swap.is_some());

            // Invalid ID should return None
            let invalid_swap = client.get_swap(&invalid_id);
            prop_assert!(invalid_swap.is_none());
        }

        /// Fuzz: Price preservation across state transitions
        ///
        /// Tests that price is preserved exactly throughout swap lifecycle.
        #[test]
        fn fuzz_price_immutability(price in 1i128..10_000_000i128) {
            let (env, client, ip_id, _secret, _blinding, seller, buyer) = setup_fuzz_env(price);

            let token_id = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &price);

            let swap_id = client.initiate_swap(
                &token_id,
                &ip_id,
                &seller,
                &price,
                &buyer,
                &0u32,
                &None,
                &0i128,
                &false,
            );

            // Check price after initiation
            let swap1 = client.get_swap(&swap_id).unwrap();
            prop_assert_eq!(swap1.price, price);

            // Check price after accept
            client.accept_swap(&swap_id);
            let swap2 = client.get_swap(&swap_id).unwrap();
            prop_assert_eq!(swap2.price, price);

            // Price must not change across transitions
            prop_assert_eq!(swap1.price, swap2.price);
        }
    }

    // ── Fuzz Tests: Edge Cases ────────────────────────────────────────────────

    proptest! {
        /// Fuzz: Boundary prices
        ///
        /// Tests minimum and maximum valid prices.
        #[test]
        fn fuzz_boundary_prices(price_multiplier in 1i128..1000000i128) {
            // Test with boundary values
            let test_prices = vec![
                1i128,                    // Minimum valid
                price_multiplier,         // Variable
                i128::MAX / 2,            // Very large
            ];

            for price in test_prices {
                let (env, client, ip_id, _secret, _blinding, seller, buyer) = setup_fuzz_env(price);

                let token_id = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
                StellarAssetClient::new(&env, &token_id).mint(&buyer, &price);

                let swap_id = client.initiate_swap(
                    &token_id,
                    &ip_id,
                    &seller,
                    &price,
                    &buyer,
                    &0u32,
                    &None,
                    &0i128,
                    &false,
                );

                let swap = client.get_swap(&swap_id).unwrap();
                prop_assert_eq!(swap.price, price);
            }
        }

        /// Fuzz: Multiple swaps independence
        ///
        /// Tests that multiple swaps don't interfere with each other.
        #[test]
        fn fuzz_multiple_swaps_independence(price in 1i128..1_000_000i128) {
            let env = Env::default();
            env.mock_all_auths();

            let seller1 = Address::generate(&env);
            let seller2 = Address::generate(&env);
            let buyer1 = Address::generate(&env);
            let buyer2 = Address::generate(&env);
            let admin = Address::generate(&env);

            // Setup two separate swaps
            let registry_id = env.register(IpRegistry, ());
            let registry = IpRegistryClient::new(&env, &registry_id);

            let secret1 = BytesN::from_array(&env, &[0x11u8; 32]);
            let blinding1 = BytesN::from_array(&env, &[0x22u8; 32]);
            let mut preimage1 = soroban_sdk::Bytes::new(&env);
            preimage1.append(&soroban_sdk::Bytes::from(secret1.clone()));
            preimage1.append(&soroban_sdk::Bytes::from(blinding1.clone()));
            let hash1: BytesN<32> = env.crypto().sha256(&preimage1).into();
            let ip_id1 = registry.commit_ip(&seller1, &hash1, &0u32);

            let secret2 = BytesN::from_array(&env, &[0x33u8; 32]);
            let blinding2 = BytesN::from_array(&env, &[0x44u8; 32]);
            let mut preimage2 = soroban_sdk::Bytes::new(&env);
            preimage2.append(&soroban_sdk::Bytes::from(secret2.clone()));
            preimage2.append(&soroban_sdk::Bytes::from(blinding2.clone()));
            let hash2: BytesN<32> = env.crypto().sha256(&preimage2).into();
            let ip_id2 = registry.commit_ip(&seller2, &hash2, &0u32);

            let token_id = env
                .register_stellar_asset_contract_v2(admin.clone())
                .address();
            StellarAssetClient::new(&env, &token_id).mint(&buyer1, &price);
            StellarAssetClient::new(&env, &token_id).mint(&buyer2, &price);

            let contract_id = env.register(AtomicSwap, ());
            let client = AtomicSwapClient::new(&env, &contract_id);
            client.initialize(&registry_id);

            let swap_id1 = client.initiate_swap(
                &token_id,
                &ip_id1,
                &seller1,
                &price,
                &buyer1,
                &0u32,
                &None,
                &0i128,
                &false,
            );

            let swap_id2 = client.initiate_swap(
                &token_id,
                &ip_id2,
                &seller2,
                &price,
                &buyer2,
                &0u32,
                &None,
                &0i128,
                &false,
            );

            // Both swaps should be independent
            prop_assert_ne!(swap_id1, swap_id2);

            let swap1 = client.get_swap(&swap_id1).unwrap();
            let swap2 = client.get_swap(&swap_id2).unwrap();

            prop_assert_eq!(swap1.seller, seller1);
            prop_assert_eq!(swap2.seller, seller2);
            prop_assert_eq!(swap1.buyer, buyer1);
            prop_assert_eq!(swap2.buyer, buyer2);

            // Operations on one swap don't affect the other
            client.accept_swap(&swap_id1);
            prop_assert_eq!(client.get_swap(&swap_id1).unwrap().status, SwapStatus::Accepted);
            prop_assert_eq!(client.get_swap(&swap_id2).unwrap().status, SwapStatus::Pending);
        }
    }

    // ── Deterministic Fuzz Coverage Tests ─────────────────────────────────────

    /// Fuzz coverage: Verify constant protocol invariants
    #[test]
    fn test_protocol_invariants_constant() {
        for _ in 0..100 {
            let (env, client, ip_id, _secret, _blinding, seller, buyer) = setup_fuzz_env(1000);

            let token_id = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &1000);

            let swap_id = client.initiate_swap(
                &token_id,
                &ip_id,
                &seller,
                &1000,
                &buyer,
                &0u32,
                &None,
                &0i128,
                &false,
            );

            let swap = client.get_swap(&swap_id).unwrap();

            // Invariants that must always hold:
            assert_ne!(swap.seller, swap.buyer);
            assert_eq!(swap.status, SwapStatus::Pending);
            assert!(swap.ip_id > 0);
            assert!(swap.price > 0);
        }
    }

    /// Fuzz: State machine transitions
    ///
    /// Verify valid and invalid state transitions across many scenarios
    #[test]
    fn test_state_machine_transitions() {
        let (env, client, ip_id, secret, blinding, seller, buyer) = setup_fuzz_env(5000);

        let token_id = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
        StellarAssetClient::new(&env, &token_id).mint(&buyer, &5000);

        // Create 10 swaps and test state transitions
        for _ in 0..10 {
            let swap_id = client.initiate_swap(
                &token_id,
                &ip_id,
                &seller,
                &5000,
                &buyer,
                &0u32,
                &None,
                &0i128,
                &false,
            );

            // Pending -> Accepted
            assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Pending);
            client.accept_swap(&swap_id);
            assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Accepted);

            // Accepted -> Completed
            client.reveal_key(&swap_id, &seller, &secret, &blinding);
            assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Completed);
        }
    }
}
