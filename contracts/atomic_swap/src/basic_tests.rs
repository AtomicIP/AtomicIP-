#[cfg(test)]
mod tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::{Client as TokenClient, StellarAssetClient},
        Address, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, DataKey, SwapStatus};
    use crate::tests::setup_token;

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Register IpRegistry, commit an IP with a known secret+blinding_factor.
    /// Returns (registry_id, ip_id, secret, blinding_factor).
    fn setup_registry(
        env: &Env,
        owner: &Address,
    ) -> (Address, u64, BytesN<32>, BytesN<32>) {
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);

        let secret = BytesN::from_array(env, &[2u8; 32]);
        let blinding_factor = BytesN::from_array(env, &[3u8; 32]);

        let mut preimage = soroban_sdk::Bytes::new(env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding_factor.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

        let ip_id = registry.commit_ip(owner, &commitment_hash);
        (registry_id, ip_id, secret, blinding_factor)
    }

    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    fn setup_swap(env: &Env, registry_id: &Address) -> Address {
        let contract_id = env.register(AtomicSwap, ());
        AtomicSwapClient::new(env, &contract_id).initialize(registry_id);
        contract_id
    }

    // ── Initialize tests ──────────────────────────────────────────────────────

    #[test]
    #[should_panic(expected = "Error(Contract, #16)")]
    fn test_initialize_twice_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let registry_id = env.register(IpRegistry, ());
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);
        client.initialize(&registry_id); // must panic AlreadyInitialized
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #15)")]
    fn test_initiate_swap_without_initialize_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (_, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initiate_swap(&token_id, &ip_id, &seller, &100_i128, &buyer);
    }

    // ── Basic unit tests ──────────────────────────────────────────────────────

    #[test]
    fn test_basic_functionality() {
        let env = Env::default();
        let buyer = Address::generate(&env);
        let buyer2 = Address::generate(&env);
        assert_ne!(buyer, buyer2);
    }

    #[test]
    fn test_storage_keys() {
        let key = DataKey::Swap(1);
        let key2 = DataKey::Swap(2);
        assert_ne!(key, key2);
        assert_ne!(key, DataKey::NextId);
    }

    #[test]
    fn test_swap_status_enum() {
        assert_ne!(SwapStatus::Pending, SwapStatus::Accepted);
        assert_ne!(SwapStatus::Accepted, SwapStatus::Completed);
        assert_ne!(SwapStatus::Completed, SwapStatus::Cancelled);
        assert_ne!(SwapStatus::Cancelled, SwapStatus::Pending);
    }

    // ── Lifecycle tests ───────────────────────────────────────────────────────

    #[test]
    fn test_initiate_swap_records_seller_correctly() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = soroban_sdk::Address::generate(&env);
        let buyer = soroban_sdk::Address::generate(&env);
        let admin = soroban_sdk::Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &500_i128, &buyer);

        let swap = client.get_swap(&swap_id).expect("swap should exist");
        assert_eq!(swap.seller, seller);
        assert_ne!(swap.seller, contract_id);
    }

    #[test]
    fn test_initiate_swap_seller_matches_caller() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let price = 1000;
        let ip_id = 1;

        // Test that we can create SwapRecord struct
        let token = Address::generate(&env);
        let swap = crate::SwapRecord {
            ip_registry_id: Address::generate(&env),
            ip_id,
            seller: seller.clone(),
            buyer: buyer.clone(),
            price,
            token,
            expiry: 0,
            status: crate::SwapStatus::Pending,
        };

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.seller, seller);
        assert_ne!(swap.seller, contract_id);
    }

    #[test]
    fn test_full_swap_lifecycle_initiate_accept_reveal_completed() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding_factor) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let token_id = setup_token(&env, &seller, &buyer, 1000);
        let swap_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &500_i128, &buyer);

        // attacker != seller — must panic with "only the seller can reveal the key"
        client.reveal_key(&swap_id, &attacker, &BytesN::from_array(&env, &[0u8; 32]), &BytesN::from_array(&env, &[0u8; 32]));
    }

    /// Escrow: payment moves buyer→contract on accept, contract→seller on reveal.
    #[test]
    fn test_escrow_held_on_accept_released_on_reveal() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding_factor) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);

        let (registry_id, ip_id, secret, blinding_factor) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1000);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        // 1. Initiate
        let swap_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &500_i128, &buyer);
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Pending);
        assert_eq!(swap.seller, seller);
        assert_eq!(swap.buyer, buyer);

        client.accept_swap(&swap_id);
        assert_eq!(token_client.balance(&buyer), 0);
        assert_eq!(token_client.balance(&swap_contract), 500);

        // 3. Reveal key → Completed
        client.reveal_key(&swap_id, &seller, &secret, &blinding_factor);
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Completed);
    }

    /// Escrow: payment refunded to buyer on cancel_expired_swap.
    #[test]
    fn test_escrow_refunded_on_cancel_expired_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1000);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &500_i128, &buyer);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &300_i128, &buyer);
        client.accept_swap(&swap_id);

        assert_eq!(token_client.balance(&buyer), 0);
        assert_eq!(token_client.balance(&swap_contract), 300);

        // Advance past expiry (7 days = 604800 seconds)
        env.ledger().with_mut(|l| l.timestamp += 604801);

        client.cancel_expired_swap(&swap_id, &buyer);
        assert_eq!(token_client.balance(&swap_contract), 0);
        assert_eq!(token_client.balance(&buyer), 300);
    }

    // ── Security tests ────────────────────────────────────────────────────────

    #[test]
    #[should_panic(expected = "Error(Contract, #4)")]
    fn test_initiate_swap_rejects_non_owner_seller() {
        let env = Env::default();
        env.mock_all_auths();

        let real_owner = Address::generate(&env);
        let attacker = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &real_owner);
        let token_id = setup_token(&env, &real_owner, &buyer, 1000);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        // attacker is not the IP owner — must panic
        client.initiate_swap(&registry_id, &token_id, &ip_id, &attacker, &500_i128, &buyer);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #7)")]
    fn test_unauthorized_reveal_key_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = soroban_sdk::Address::generate(&env);
        let buyer = soroban_sdk::Address::generate(&env);
        let attacker = soroban_sdk::Address::generate(&env);

        let (registry_id, ip_id, secret, blinding_factor) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1000);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        // 1. Initiate and accept the swap
        let swap_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &500_i128, &buyer);
        client.accept_swap(&swap_id);
        // attacker != seller — must panic with ContractError(7)
        client.reveal_key(&swap_id, &attacker, &secret, &blinding_factor);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #9)")]
    fn test_unauthorized_cancel_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let attacker = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env, &registry_id));
        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer);
        // attacker is neither seller nor buyer — must panic with ContractError(9)
        client.cancel_swap(&swap_id, &attacker);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_reveal_key_invalid_key_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1000);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &500_i128, &buyer);
        client.accept_swap(&swap_id);

        let garbage = BytesN::from_array(&env, &[0xffu8; 32]);
        client.reveal_key(&swap_id, &seller, &garbage, &garbage);
    }

    #[test]
    fn test_reveal_key_valid_key_completes_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding_factor) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1000);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &500_i128, &buyer);
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding_factor);

        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Completed
        );
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #11)")]
    fn test_cancel_expired_swap_pending_state_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1000);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &500_i128, &buyer);
        client.cancel_expired_swap(&swap_id, &buyer);
    }

    /// Issue #71: initiate_swap with non-existent ip_id should panic
    #[test]
    #[should_panic(expected = "HostError: Error(Contract, #1)")]
    fn test_initiate_swap_with_non_existent_ip_id_panics() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = soroban_sdk::Address::generate(&env);
        let buyer = soroban_sdk::Address::generate(&env);
        let admin = soroban_sdk::Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let token_id = env.register_stellar_asset_contract(admin.clone());
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        // ip_id 9999 does not exist in the registry — must panic with IpNotFound (code 1)
        client.initiate_swap(&registry_id, &token_id, &9999u64, &seller, &500_i128, &buyer);
    }

    /// Issue #53: reveal_key must emit a KeyRevealedEvent.
    #[test]
    fn test_reveal_key_emits_event() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = soroban_sdk::Address::generate(&env);
        let buyer = soroban_sdk::Address::generate(&env);

        let (registry_id, ip_id, secret, blinding_factor) = setup_registry(&env, &seller);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&registry_id, &ip_id, &seller, &500_i128, &buyer);
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding_factor);

        let all_events = env.events().all();
        let event = all_events.last().unwrap();

        let expected_topics = (soroban_sdk::symbol_short!("key_rev"),).into_val(&env);
        assert_eq!(event.1, expected_topics);

        let observed: KeyRevealedEvent = soroban_sdk::FromVal::from_val(&env, &event.2);
        assert_eq!(observed.swap_id, swap_id);
        assert_eq!(observed.decryption_key, secret);
    }

    /// Issue #54: cancel_swap must emit a SwapCancelledEvent.
    #[test]
    fn test_cancel_swap_emits_event() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = soroban_sdk::Address::generate(&env);
        let buyer = soroban_sdk::Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&registry_id, &ip_id, &seller, &500_i128, &buyer);
        client.cancel_swap(&swap_id, &seller);

        let all_events = env.events().all();
        let event = all_events.last().unwrap();

        let expected_topics = (soroban_sdk::symbol_short!("swap_cncl"),).into_val(&env);
        assert_eq!(event.1, expected_topics);

        let observed: SwapCancelledEvent = soroban_sdk::FromVal::from_val(&env, &event.2);
        assert_eq!(observed.swap_id, swap_id);
        assert_eq!(observed.canceller, seller);
    }
}
