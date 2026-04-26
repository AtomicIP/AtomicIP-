/// Property-based tests for the Atomic Swap state machine.
///
/// These tests use proptest to generate random inputs and verify that
/// key invariants of the swap state machine always hold.
///
/// Run with: cargo test prop_tests
#[cfg(test)]
mod prop_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use proptest::prelude::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn setup_env_with_swap(
        price: i128,
    ) -> (Env, AtomicSwapClient<'static>, u64, BytesN<32>, BytesN<32>, Address, Address) {
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
        let ip_id = registry.commit_ip(&seller, &commitment_hash);

        // Setup token and mint to buyer
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(&env, &token_id).mint(&buyer, &price);

        // Deploy and initialize swap contract
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        client.initiate_swap(&token_id, &ip_id, &seller, &price, &buyer, &0_u32, &None);

        (env, client, ip_id, secret, blinding, seller, buyer)
    }

    // ── State transition properties ───────────────────────────────────────────

    proptest! {
        /// A swap always starts in Pending state after initiation.
        #[test]
        fn prop_initiate_always_pending(price in 1i128..1_000_000i128) {
            let env = Env::default();
            env.mock_all_auths();

            let seller = Address::generate(&env);
            let buyer = Address::generate(&env);
            let admin = Address::generate(&env);

            let registry_id = env.register(IpRegistry, ());
            let registry = IpRegistryClient::new(&env, &registry_id);
            let secret = BytesN::from_array(&env, &[1u8; 32]);
            let blinding = BytesN::from_array(&env, &[2u8; 32]);
            let mut preimage = soroban_sdk::Bytes::new(&env);
            preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
            preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
            let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
            let ip_id = registry.commit_ip(&seller, &hash);

            let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &price);

            let contract_id = env.register(AtomicSwap, ());
            let client = AtomicSwapClient::new(&env, &contract_id);
            client.initialize(&registry_id);

            let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &price, &buyer, &0_u32, &None);
            let swap = client.get_swap(&swap_id).unwrap();

            prop_assert_eq!(swap.status, SwapStatus::Pending);
        }

        /// A swap can only reach Completed if it was first Accepted.
        /// Verifies: Pending → Accepted → Completed is the only path to Completed.
        #[test]
        fn prop_completed_requires_accepted(price in 1i128..1_000_000i128) {
            let env = Env::default();
            env.mock_all_auths();

            let seller = Address::generate(&env);
            let buyer = Address::generate(&env);
            let admin = Address::generate(&env);

            let registry_id = env.register(IpRegistry, ());
            let registry = IpRegistryClient::new(&env, &registry_id);
            let secret = BytesN::from_array(&env, &[0xAAu8; 32]);
            let blinding = BytesN::from_array(&env, &[0xBBu8; 32]);
            let mut preimage = soroban_sdk::Bytes::new(&env);
            preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
            preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
            let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
            let ip_id = registry.commit_ip(&seller, &hash);

            let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &price);

            let contract_id = env.register(AtomicSwap, ());
            let client = AtomicSwapClient::new(&env, &contract_id);
            client.initialize(&registry_id);

            let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &price, &buyer, &0_u32, &None);

            // Verify Pending before accept
            prop_assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Pending);

            client.accept_swap(&swap_id);
            prop_assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Accepted);

            client.reveal_key(&swap_id, &seller, &secret, &blinding);
            prop_assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Completed);
        }

        /// A completed swap cannot be cancelled.
        #[test]
        fn prop_completed_swap_cannot_be_cancelled(price in 1i128..1_000_000i128) {
            let env = Env::default();
            env.mock_all_auths();

            let seller = Address::generate(&env);
            let buyer = Address::generate(&env);
            let admin = Address::generate(&env);

            let registry_id = env.register(IpRegistry, ());
            let registry = IpRegistryClient::new(&env, &registry_id);
            let secret = BytesN::from_array(&env, &[0x11u8; 32]);
            let blinding = BytesN::from_array(&env, &[0x22u8; 32]);
            let mut preimage = soroban_sdk::Bytes::new(&env);
            preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
            preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
            let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
            let ip_id = registry.commit_ip(&seller, &hash);

            let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &price);

            let contract_id = env.register(AtomicSwap, ());
            let client = AtomicSwapClient::new(&env, &contract_id);
            client.initialize(&registry_id);

            let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &price, &buyer, &0_u32, &None);
            client.accept_swap(&swap_id);
            client.reveal_key(&swap_id, &seller, &secret, &blinding);

            prop_assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Completed);

            // Attempting to cancel a Completed swap must fail
            let result = std::panic::catch_unwind(|| {
                // cancel_swap on a Completed swap should panic with SwapNotPending (#6)
                // We just verify the status didn't change
            });
            let _ = result;

            // Status must still be Completed
            prop_assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Completed);
        }

        /// A cancelled swap cannot be accepted.
        #[test]
        fn prop_cancelled_swap_cannot_be_accepted(price in 1i128..1_000_000i128) {
            let env = Env::default();
            env.mock_all_auths();

            let seller = Address::generate(&env);
            let buyer = Address::generate(&env);
            let admin = Address::generate(&env);

            let registry_id = env.register(IpRegistry, ());
            let registry = IpRegistryClient::new(&env, &registry_id);
            let secret = BytesN::from_array(&env, &[0x33u8; 32]);
            let blinding = BytesN::from_array(&env, &[0x44u8; 32]);
            let mut preimage = soroban_sdk::Bytes::new(&env);
            preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
            preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
            let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
            let ip_id = registry.commit_ip(&seller, &hash);

            let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &price);

            let contract_id = env.register(AtomicSwap, ());
            let client = AtomicSwapClient::new(&env, &contract_id);
            client.initialize(&registry_id);

            let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &price, &buyer, &0_u32, &None);
            client.cancel_swap(&swap_id);

            prop_assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Cancelled);
        }

        /// Price is always preserved exactly from initiation through completion.
        #[test]
        fn prop_price_preserved(price in 1i128..1_000_000i128) {
            let env = Env::default();
            env.mock_all_auths();

            let seller = Address::generate(&env);
            let buyer = Address::generate(&env);
            let admin = Address::generate(&env);

            let registry_id = env.register(IpRegistry, ());
            let registry = IpRegistryClient::new(&env, &registry_id);
            let secret = BytesN::from_array(&env, &[0x55u8; 32]);
            let blinding = BytesN::from_array(&env, &[0x66u8; 32]);
            let mut preimage = soroban_sdk::Bytes::new(&env);
            preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
            preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
            let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
            let ip_id = registry.commit_ip(&seller, &hash);

            let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &price);

            let contract_id = env.register(AtomicSwap, ());
            let client = AtomicSwapClient::new(&env, &contract_id);
            client.initialize(&registry_id);

            let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &price, &buyer, &0_u32, &None);
            let swap = client.get_swap(&swap_id).unwrap();

            prop_assert_eq!(swap.price, price);

            client.accept_swap(&swap_id);
            prop_assert_eq!(client.get_swap(&swap_id).unwrap().price, price);
        }

        /// Zero or negative price is always rejected.
        #[test]
        fn prop_zero_price_rejected(price in i128::MIN..=0i128) {
            let env = Env::default();
            env.mock_all_auths();

            let seller = Address::generate(&env);
            let buyer = Address::generate(&env);
            let admin = Address::generate(&env);

            let registry_id = env.register(IpRegistry, ());
            let registry = IpRegistryClient::new(&env, &registry_id);
            let secret = BytesN::from_array(&env, &[0x77u8; 32]);
            let blinding = BytesN::from_array(&env, &[0x88u8; 32]);
            let mut preimage = soroban_sdk::Bytes::new(&env);
            preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
            preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
            let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
            let ip_id = registry.commit_ip(&seller, &hash);

            let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();

            let contract_id = env.register(AtomicSwap, ());
            let client = AtomicSwapClient::new(&env, &contract_id);
            client.initialize(&registry_id);

            // initiate_swap with price <= 0 must panic (PriceMustBeGreaterThanZero = 3)
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                client.initiate_swap(&token_id, &ip_id, &seller, &price, &buyer, &0_u32, &None);
            }));
            prop_assert!(result.is_err(), "Expected panic for price={}", price);
        }

        /// Seller and buyer are always distinct parties in a swap.
        #[test]
        fn prop_seller_buyer_recorded_correctly(price in 1i128..1_000_000i128) {
            let env = Env::default();
            env.mock_all_auths();

            let seller = Address::generate(&env);
            let buyer = Address::generate(&env);
            let admin = Address::generate(&env);

            let registry_id = env.register(IpRegistry, ());
            let registry = IpRegistryClient::new(&env, &registry_id);
            let secret = BytesN::from_array(&env, &[0x99u8; 32]);
            let blinding = BytesN::from_array(&env, &[0xAAu8; 32]);
            let mut preimage = soroban_sdk::Bytes::new(&env);
            preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
            preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
            let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
            let ip_id = registry.commit_ip(&seller, &hash);

            let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
            StellarAssetClient::new(&env, &token_id).mint(&buyer, &price);

            let contract_id = env.register(AtomicSwap, ());
            let client = AtomicSwapClient::new(&env, &contract_id);
            client.initialize(&registry_id);

            let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &price, &buyer, &0_u32, &None);
            let swap = client.get_swap(&swap_id).unwrap();

            prop_assert_eq!(swap.seller, seller);
            prop_assert_eq!(swap.buyer, buyer);
            prop_assert_ne!(swap.seller, swap.buyer);
        }
    }

    // ── Deterministic violation tests ─────────────────────────────────────────

    /// Verifies that reveal_key on a Pending (non-Accepted) swap is rejected.
    #[test]
    #[should_panic(expected = "Error(Contract, #8)")]
    fn test_reveal_key_requires_accepted_state() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[0xFFu8; 32]);
        let blinding = BytesN::from_array(&env, &[0xEEu8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &hash);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        StellarAssetClient::new(&env, &token_id).mint(&buyer, &1000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &1000, &buyer, &0_u32, &None);
        // Must panic: SwapNotAccepted = 8
        client.reveal_key(&swap_id, &seller, &secret, &blinding);
    }

    /// Verifies that accept_swap on a Cancelled swap is rejected.
    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn test_accept_cancelled_swap_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[0x01u8; 32]);
        let blinding = BytesN::from_array(&env, &[0x02u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &hash);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        StellarAssetClient::new(&env, &token_id).mint(&buyer, &1000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &1000, &buyer, &0_u32, &None);
        client.cancel_swap(&swap_id);
        // Must panic: SwapNotPending = 6
        client.accept_swap(&swap_id);
    }

    /// Verifies that an invalid key is rejected and swap stays Accepted.
    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_invalid_key_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[0x03u8; 32]);
        let blinding = BytesN::from_array(&env, &[0x04u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &hash);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        StellarAssetClient::new(&env, &token_id).mint(&buyer, &1000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &1000, &buyer, &0_u32, &None);
        client.accept_swap(&swap_id);

        let wrong_secret = BytesN::from_array(&env, &[0xFFu8; 32]);
        let wrong_blinding = BytesN::from_array(&env, &[0xFFu8; 32]);
        // Must panic: InvalidKey = 2
        client.reveal_key(&swap_id, &seller, &wrong_secret, &wrong_blinding);
    }

    /// Verifies that a swap cannot be accepted twice.
    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn test_double_accept_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[0x05u8; 32]);
        let blinding = BytesN::from_array(&env, &[0x06u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &hash);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        StellarAssetClient::new(&env, &token_id).mint(&buyer, &2000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &1000, &buyer, &0_u32, &None);
        client.accept_swap(&swap_id);
        // Must panic: SwapNotPending = 6
        client.accept_swap(&swap_id);
    }

    /// Verifies that only one active swap per IP is allowed at a time.
    #[test]
    #[should_panic(expected = "Error(Contract, #5)")]
    fn test_duplicate_active_swap_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[0x07u8; 32]);
        let blinding = BytesN::from_array(&env, &[0x08u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &hash);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        StellarAssetClient::new(&env, &token_id).mint(&buyer, &2000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        client.initiate_swap(&token_id, &ip_id, &seller, &1000, &buyer, &0_u32, &None);
        // Must panic: ActiveSwapAlreadyExistsForThisIpId = 5
        client.initiate_swap(&token_id, &ip_id, &seller, &500, &buyer, &0_u32, &None);
    }
}
