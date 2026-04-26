#[cfg(test)]
mod tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Events, Ledger},
        token::{Client as TokenClient, StellarAssetClient},
        Address, BytesN, Env, IntoVal,
    };

    use crate::{AtomicSwap, AtomicSwapClient, DataKey, SwapStatus, KeyRevealedEvent, SwapCancelledEvent};

    // ── Helpers ───────────────────────────────────────────────────────────────

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

    fn setup_swap(env: &Env, registry_id: &Address) -> Address {
        let contract_id = env.register(AtomicSwap, ());
        AtomicSwapClient::new(env, &contract_id).initialize(registry_id);
        contract_id
    }

    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    // ── Initialize tests ──────────────────────────────────────────────────────

    #[test]
    #[should_panic(expected = "Error(Contract, #22)")]
    fn test_initialize_twice_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let registry_id = env.register(IpRegistry, ());
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);
        client.initialize(&registry_id); // must panic AlreadyInitialized
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

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);

        let swap = client.get_swap(&swap_id).expect("swap should exist");
        assert_eq!(swap.seller, seller);
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

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding_factor);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Completed);
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

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);
        let token_client = TokenClient::new(&env, &token_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);

        client.accept_swap(&swap_id);
        assert_eq!(token_client.balance(&buyer), 0);
        assert_eq!(token_client.balance(&contract_id), 500);

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
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);
        let token_client = TokenClient::new(&env, &token_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &300_i128, &buyer, &0_u32, &None);
        client.accept_swap(&swap_id);

        assert_eq!(token_client.balance(&buyer), 700);
        assert_eq!(token_client.balance(&contract_id), 300);

        // Advance past expiry (7 days = 604800 seconds)
        env.ledger().with_mut(|l| l.timestamp += 604801);

        client.cancel_expired_swap(&swap_id, &buyer);
        assert_eq!(token_client.balance(&contract_id), 0);
        assert_eq!(token_client.balance(&buyer), 1000);
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
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        // attacker is not the IP owner — must panic
        client.initiate_swap(&token_id, &ip_id, &attacker, &500_i128, &buyer, &0_u32, &None);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #7)")]
    fn test_unauthorized_reveal_key_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let attacker = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, secret, blinding_factor) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);
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
        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);
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
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);
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
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding_factor);

        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Completed
        );
    }

    /// Issue #74: cancel_swap after reveal_key should panic (swap already finalised).
    #[test]
    #[should_panic(expected = "Error(Contract, #10)")]
    fn test_cancel_swap_after_reveal_key_panics() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding_factor) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding_factor);

        // Swap is Completed, cancel_swap should panic.
        client.cancel_swap(&swap_id, &seller);
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
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);
        client.cancel_expired_swap(&swap_id, &buyer);
    }

    /// Issue #53: reveal_key must emit a KeyRevealedEvent.
    #[test]
    fn test_reveal_key_emits_event() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, secret, blinding_factor) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        client.initialize(&registry_id);
        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding_factor);

        let all_events = env.events().all();
        let event = all_events.last().unwrap();

        assert_eq!(event.0.get_unchecked(0), soroban_sdk::symbol_short!("key_rev"));

        let observed: KeyRevealedEvent = soroban_sdk::FromVal::from_val(&env, &event.2);
        assert_eq!(observed.swap_id, swap_id);
    }

    /// Issue #54: cancel_swap must emit a SwapCancelledEvent.
    #[test]
    fn test_cancel_swap_emits_event() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);
        client.cancel_swap(&swap_id, &seller);

        let all_events = env.events().all();
        let event = all_events.last().unwrap();

        assert_eq!(event.0.get_unchecked(0), soroban_sdk::symbol_short!("swap_cncl"));

        let observed: SwapCancelledEvent = soroban_sdk::FromVal::from_val(&env, &event.2);
        assert_eq!(observed.swap_id, swap_id);
        assert_eq!(observed.canceller, seller);
    }

    // ── #251: cancel_pending_swap ─────────────────────────────────────────────

    #[test]
    fn test_cancel_pending_swap_after_expiry_succeeds() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env, &registry_id));
        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);

        // Advance past expiry
        env.ledger().with_mut(|l| l.timestamp += 604801);

        client.cancel_pending_swap(&swap_id, &buyer);
        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Cancelled);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #25)")]
    fn test_cancel_pending_swap_before_expiry_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env, &registry_id));
        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);

        // Not expired yet — must panic with PendingSwapNotExpired (#25)
        client.cancel_pending_swap(&swap_id, &buyer);
    }

    // ── #252: extend_swap_expiry ──────────────────────────────────────────────

    #[test]
    fn test_extend_swap_expiry_succeeds() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env, &registry_id));
        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);

        let old_expiry = client.get_swap(&swap_id).unwrap().expiry;
        let new_expiry = old_expiry + 86400;

        client.extend_swap_expiry(&swap_id, &new_expiry);
        assert_eq!(client.get_swap(&swap_id).unwrap().expiry, new_expiry);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #26)")]
    fn test_extend_swap_expiry_not_greater_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env, &registry_id));
        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);

        let old_expiry = client.get_swap(&swap_id).unwrap().expiry;
        // Same expiry — must panic with NewExpiryNotGreater (#26)
        client.extend_swap_expiry(&swap_id, &old_expiry);
    }

    // ── #253: swap history ────────────────────────────────────────────────────

    #[test]
    fn test_swap_history_tracks_state_transitions() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env, &registry_id));
        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        let history = client.get_swap_history(&swap_id);
        assert_eq!(history.len(), 3);
        assert_eq!(history.get(0).unwrap().status, SwapStatus::Pending);
        assert_eq!(history.get(1).unwrap().status, SwapStatus::Accepted);
        assert_eq!(history.get(2).unwrap().status, SwapStatus::Completed);
    }

    #[test]
    fn test_swap_history_on_cancellation() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env, &registry_id));
        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);
        client.cancel_swap(&swap_id, &seller);

        let history = client.get_swap_history(&swap_id);
        assert_eq!(history.len(), 2);
        assert_eq!(history.get(0).unwrap().status, SwapStatus::Pending);
        assert_eq!(history.get(1).unwrap().status, SwapStatus::Cancelled);
    }

    // ── #254: multi-sig approval ──────────────────────────────────────────────

    #[test]
    fn test_approve_swap_and_accept_with_required_approvals() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let approver = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env, &registry_id));
        // Require 1 approval
        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &1_u32, &None);

        client.approve_swap(&swap_id, &approver);
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Completed);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #27)")]
    fn test_accept_swap_without_required_approvals_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env, &registry_id));
        // Require 2 approvals but provide none
        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &2_u32, &None);

        // Must panic with InsufficientApprovals (#27)
        client.accept_swap(&swap_id);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #28)")]
    fn test_duplicate_approval_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let approver = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env, &registry_id));
        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &2_u32, &None);

        client.approve_swap(&swap_id, &approver);
        // Same approver again — must panic with AlreadyApproved (#28)
        client.approve_swap(&swap_id, &approver);
    }

    // ── #309: batch_initiate_swap ─────────────────────────────────────────────

    #[test]
    fn test_batch_initiate_swap_returns_correct_ids() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let ip_id_0 = registry.commit_ip(&seller, &BytesN::from_array(&env, &[10u8; 32]));
        let ip_id_1 = registry.commit_ip(&seller, &BytesN::from_array(&env, &[11u8; 32]));

        let token_id = setup_token(&env, &admin, &buyer, 3000);
        let client = AtomicSwapClient::new(&env, &setup_swap(&env, &registry_id));

        let ip_ids = soroban_sdk::vec![&env, ip_id_0, ip_id_1];
        let prices = soroban_sdk::vec![&env, 1000_i128, 500_i128];

        let swap_ids = client.batch_initiate_swap(&token_id, &ip_ids, &seller, &prices, &buyer, &0_u32, &None);

        assert_eq!(swap_ids.len(), 2);
        assert_ne!(swap_ids.get(0).unwrap(), swap_ids.get(1).unwrap());
        assert_eq!(client.get_swap(&swap_ids.get(0).unwrap()).unwrap().ip_id, ip_id_0);
        assert_eq!(client.get_swap(&swap_ids.get(1).unwrap()).unwrap().ip_id, ip_id_1);
        assert_eq!(client.get_swap(&swap_ids.get(0).unwrap()).unwrap().price, 1000);
        assert_eq!(client.get_swap(&swap_ids.get(1).unwrap()).unwrap().price, 500);
    }

    #[test]
    fn test_batch_initiate_swap_all_pending() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let ip_id_0 = registry.commit_ip(&seller, &BytesN::from_array(&env, &[20u8; 32]));
        let ip_id_1 = registry.commit_ip(&seller, &BytesN::from_array(&env, &[21u8; 32]));
        let ip_id_2 = registry.commit_ip(&seller, &BytesN::from_array(&env, &[22u8; 32]));

        let token_id = setup_token(&env, &admin, &buyer, 5000);
        let client = AtomicSwapClient::new(&env, &setup_swap(&env, &registry_id));

        let ip_ids = soroban_sdk::vec![&env, ip_id_0, ip_id_1, ip_id_2];
        let prices = soroban_sdk::vec![&env, 100_i128, 200_i128, 300_i128];

        let swap_ids = client.batch_initiate_swap(&token_id, &ip_ids, &seller, &prices, &buyer, &0_u32, &None);

        for i in 0..swap_ids.len() {
            let swap = client.get_swap(&swap_ids.get(i).unwrap()).unwrap();
            assert_eq!(swap.status, SwapStatus::Pending);
            assert_eq!(swap.seller, seller);
            assert_eq!(swap.buyer, buyer);
        }
    }

    // ── #311: Referral rewards ────────────────────────────────────────────────

    #[test]
    fn test_referral_reward_paid_on_reveal() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let referrer = Address::generate(&env);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        let (registry_id, ip_id, secret, blinding_factor) = setup_registry(&env, &seller);
        let price = 1000_i128;
        let token_id = setup_token(&env, &admin, &buyer, price);
        let token_client = TokenClient::new(&env, &token_id);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        // 100 bps protocol fee, 50 bps referral fee
        client.admin_set_protocol_config(&100_u32, &treasury, &86400u64, &2_592_000u64, &50_u32);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &price, &buyer, &0_u32, &Some(referrer.clone()));
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding_factor);

        // protocol fee = 1000 * 100 / 10000 = 10
        // referral fee = 1000 * 50 / 10000 = 5
        // seller gets = 1000 - 10 - 5 = 985
        assert_eq!(token_client.balance(&referrer), 5);
        assert_eq!(token_client.balance(&treasury), 10);
        assert_eq!(token_client.balance(&seller), 985);
    }

    #[test]
    fn test_no_referral_reward_when_referrer_is_none() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);

        let (registry_id, ip_id, secret, blinding_factor) = setup_registry(&env, &seller);
        let price = 1000_i128;
        let token_id = setup_token(&env, &admin, &buyer, price);
        let token_client = TokenClient::new(&env, &token_id);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        // 100 bps protocol fee, 50 bps referral fee — but no referrer set
        client.admin_set_protocol_config(&100_u32, &treasury, &86400u64, &2_592_000u64, &50_u32);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &price, &buyer, &0_u32, &None);
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding_factor);

        // seller gets = 1000 - 10 = 990 (no referral deduction)
        assert_eq!(token_client.balance(&treasury), 10);
        assert_eq!(token_client.balance(&seller), 990);
    }

    #[test]
    fn test_referral_stored_in_swap_record() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let referrer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env, &registry_id));
        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &Some(referrer.clone()));

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.referrer, Some(referrer));
    }
}

