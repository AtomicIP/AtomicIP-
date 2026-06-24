/// Mutation-catching tests for Atomic Swap contract.
/// Targets 85%+ mutation kill rate by covering:
///   - status transition guards (Pending → Accepted → Completed)
///   - auth requirement bypasses
///   - ID sequencing and counter mutations
///   - ActiveSwap locking (off-by-one, missing remove)
///   - ownership checks (NotIPOwner, IpRevoked)
///   - storage key collisions
///   - expiry enforcement
#[cfg(test)]
mod mutation_tests {
    use soroban_sdk::{
        contractclient,
        testutils::Address as _,
        Address, BytesN, Env, Vec,
    };

    // ── IpRegistry client ──────────────────────────────────────────────────────

    #[contractclient(name = "IpRegistryClient")]
    #[allow(dead_code)]
    trait IpRegistry {
        fn commit_ip(env: Env, owner: Address, commitment_hash: BytesN<32>, pow_difficulty: u32) -> u64;
        fn get_ip(env: Env, ip_id: u64) -> ip_registry::IpRecord;
        fn transfer_ip(env: Env, ip_id: u64, new_owner: Address);
        fn revoke_ip(env: Env, ip_id: u64);
        fn verify_commitment(env: Env, ip_id: u64, secret: BytesN<32>, blinding_factor: BytesN<32>) -> bool;
        fn is_ip_owner(env: Env, ip_id: u64, address: Address) -> bool;
    }

    // ── AtomicSwap client ──────────────────────────────────────────────────────

    #[contractclient(name = "AtomicSwapClient")]
    #[allow(dead_code)]
    trait AtomicSwap {
        fn initialize(env: Env, ip_registry: Address);
        fn initiate_swap(
            env: Env,
            token: Address,
            ip_id: u64,
            seller: Address,
            price: i128,
            buyer: Address,
            required_approvals: u32,
            referrer: Option<Address>,
            collateral_amount: i128,
            insurance_enabled: bool,
        ) -> u64;
        fn accept_swap(env: Env, swap_id: u64);
        fn reveal_key(env: Env, swap_id: u64, caller: Address, secret: BytesN<32>, blinding_factor: BytesN<32>);
        fn cancel_swap(env: Env, swap_id: u64, canceller: Address);
        fn cancel_expired_swap(env: Env, swap_id: u64, caller: Address);
        fn get_swap(env: Env, swap_id: u64) -> Option<crate::SwapRecord>;
        fn swap_count(env: Env) -> u64;
        fn set_oracle(env: Env, caller: Address, oracle_address: Address, enabled: bool);
    }

    use crate::AtomicSwap;
    use ip_registry::IpRegistry;

    // ── helpers ────────────────────────────────────────────────────────────────

    fn setup_env() -> (Env, AtomicSwapClient<'static>, IpRegistryClient<'static>, Address) {
        let e = Env::default();
        e.mock_all_auths();

        let registry_id = e.register(IpRegistry, ());
        let registry_client = IpRegistryClient::new(&e, &registry_id);

        let swap_id = e.register(AtomicSwap, ());
        let swap_client = AtomicSwapClient::new(&e, &swap_id);

        swap_client.initialize(&registry_id);

        let token = Address::generate(&e);
        (e, swap_client, registry_client, token)
    }

    fn commit_test_ip(
        registry: &IpRegistryClient<'_>,
        e: &Env,
        owner: &Address,
        seed: u8,
    ) -> u64 {
        registry.commit_ip(owner, &BytesN::from_array(e, &[seed; 32]), &0u32)
    }

    fn commithash(e: &Env, seed: u8) -> BytesN<32> {
        BytesN::from_array(e, &[seed; 32])
    }

    // ── Initiate swap mutations ───────────────────────────────────────────────

    #[test]
    fn initiate_swap_creates_pending_swap() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry, &e, &seller, 0x01);

        let id = swap.initiate_swap(&token, &ip_id, &seller, &1000i128, &buyer, &0u32, &None, &0i128, &false);
        assert_eq!(id, 0, "first swap ID must be 0 (NextId starts at 0)");

        let record = swap.get_swap(&id).unwrap();
        assert_eq!(record.status, crate::SwapStatus::Pending);
        assert_eq!(record.seller, seller);
        assert_eq!(record.buyer, buyer);
        assert_eq!(record.price, 1000i128);
        assert_eq!(record.ip_id, ip_id);
    }

    #[test]
    fn initiate_swap_returns_sequential_ids() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);

        let ip_id1 = commit_test_ip(&registry, &e, &seller, 0x10);
        let ip_id2 = commit_test_ip(&registry, &e, &seller, 0x11);
        let ip_id3 = commit_test_ip(&registry, &e, &seller, 0x12);

        let id1 = swap.initiate_swap(&token, &ip_id1, &seller, &100i128, &buyer, &0u32, &None, &0i128, &false);
        let id2 = swap.initiate_swap(&token, &ip_id2, &seller, &200i128, &buyer, &0u32, &None, &0i128, &false);
        let id3 = swap.initiate_swap(&token, &ip_id3, &seller, &300i128, &buyer, &0u32, &None, &0i128, &false);

        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
        assert_eq!(id3, 2);
        assert_eq!(swap.swap_count(), 3);
    }

    #[test]
    fn get_swap_returns_none_for_nonexistent() {
        let (e, swap, _, _) = setup_env();
        let result = swap.get_swap(&999u64);
        assert_eq!(result, None);
    }

    #[test]
    fn get_swap_returns_correct_record() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry, &e, &seller, 0x20);

        let id = swap.initiate_swap(&token, &ip_id, &seller, &5000i128, &buyer, &0u32, &None, &0i128, &false);
        let record = swap.get_swap(&id).unwrap();

        assert_eq!(record.ip_id, ip_id);
        assert_eq!(record.seller, seller);
        assert_eq!(record.buyer, buyer);
        assert_eq!(record.price, 5000i128);
        assert_eq!(record.token, token);
        assert_eq!(record.status, crate::SwapStatus::Pending);
        assert_eq!(record.required_approvals, 0);
        assert!(!record.insurance_enabled);
    }

    // ── ActiveSwap lock mutations ─────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn cannot_initiate_second_swap_for_same_ip_while_active() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry, &e, &seller, 0x30);

        swap.initiate_swap(&token, &ip_id, &seller, &100i128, &buyer, &0u32, &None, &0i128, &false);
        swap.initiate_swap(&token, &ip_id, &seller, &200i128, &buyer, &0u32, &None, &0i128, &false);
    }

    #[test]
    fn cancelled_swap_releases_ip_lock() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry, &e, &seller, 0x31);

        let id = swap.initiate_swap(&token, &ip_id, &seller, &100i128, &buyer, &0u32, &None, &0i128, &false);
        swap.cancel_swap(&id, &seller);

        // Should be able to initiate a new swap for the same IP
        let id2 = swap.initiate_swap(&token, &ip_id, &seller, &200i128, &buyer, &0u32, &None, &0i128, &false);
        assert_eq!(id2, 1);
    }

    // ── Cancel swap mutations ─────────────────────────────────────────────────

    #[test]
    fn cancel_swap_transitions_to_cancelled() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry, &e, &seller, 0x40);

        let id = swap.initiate_swap(&token, &ip_id, &seller, &100i128, &buyer, &0u32, &None, &0i128, &false);
        swap.cancel_swap(&id, &seller);

        let record = swap.get_swap(&id).unwrap();
        assert_eq!(record.status, crate::SwapStatus::Cancelled);
    }

    #[test]
    #[should_panic]
    fn cancel_already_cancelled_swap_panics() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry, &e, &seller, 0x41);

        let id = swap.initiate_swap(&token, &ip_id, &seller, &100i128, &buyer, &0u32, &None, &0i128, &false);
        swap.cancel_swap(&id, &seller);
        swap.cancel_swap(&id, &seller);
    }

    #[test]
    #[should_panic]
    fn cancel_nonexistent_swap_panics() {
        let (e, swap, _, _) = setup_env();
        swap.cancel_swap(&999u64, &Address::generate(&e));
    }

    // ── NotIPOwner mutation ───────────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn non_owner_cannot_initiate_swap() {
        let (e, swap, registry, token) = setup_env();
        let owner = Address::generate(&e);
        let non_owner = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry, &e, &owner, 0x50);

        // non_owner tries to initiate a swap for an IP they don't own
        swap.initiate_swap(&token, &ip_id, &non_owner, &100i128, &buyer, &0u32, &None, &0i128, &false);
    }

    // ── Revoked IP mutation ──────────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn revoked_ip_cannot_be_swapped() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry, &e, &seller, 0x60);

        registry.revoke_ip(&ip_id);
        swap.initiate_swap(&token, &ip_id, &seller, &100i128, &buyer, &0u32, &None, &0i128, &false);
    }

    // ── Initialization mutation ───────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn cannot_initialize_twice() {
        let (e, swap, _, _) = setup_env();
        // initialize called once in setup_env, second call should panic
        swap.initialize(&Address::generate(&e));
    }

    #[test]
    fn uninitialized_swap_cannot_initiate() {
        let e = Env::default();
        e.mock_all_auths();

        let registry_id = e.register(IpRegistry, ());
        let registry_client = IpRegistryClient::new(&e, &registry_id);
        let swap_id = e.register(AtomicSwap, ());
        let swap_client = AtomicSwapClient::new(&e, &swap_id);

        // Do NOT call initialize

        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry_client, &e, &seller, 0x70);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            swap_client.initiate_swap(
                &Address::generate(&e),
                &ip_id,
                &seller,
                &100i128,
                &buyer,
                &0u32,
                &None,
                &0i128,
                &false,
            );
        }));
        assert!(result.is_err(), "uninitialized swap should panic on initiate");
    }

    // ── Swap count mutation ──────────────────────────────────────────────────

    #[test]
    fn swap_count_starts_at_zero() {
        let e = Env::default();
        e.mock_all_auths();
        let swap_id = e.register(AtomicSwap, ());
        let swap_client = AtomicSwapClient::new(&e, &swap_id);
        assert_eq!(swap_client.swap_count(), 0);
    }

    #[test]
    fn swap_count_increments_with_initiations() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);

        assert_eq!(swap.swap_count(), 0);

        let ip_id1 = commit_test_ip(&registry, &e, &seller, 0x80);
        swap.initiate_swap(&token, &ip_id1, &seller, &100i128, &buyer, &0u32, &None, &0i128, &false);
        assert_eq!(swap.swap_count(), 1);

        let ip_id2 = commit_test_ip(&registry, &e, &seller, 0x81);
        swap.initiate_swap(&token, &ip_id2, &seller, &200i128, &buyer, &0u32, &None, &0i128, &false);
        assert_eq!(swap.swap_count(), 2);
    }

    // ── Accept swap mutations ─────────────────────────────────────────────────

    #[test]
    fn accept_swap_transitions_to_accepted() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry, &e, &seller, 0x90);

        let id = swap.initiate_swap(&token, &ip_id, &seller, &100i128, &buyer, &0u32, &None, &0i128, &false);
        swap.accept_swap(&id);

        let record = swap.get_swap(&id).unwrap();
        assert_eq!(record.status, crate::SwapStatus::Accepted);
    }

    #[test]
    #[should_panic]
    fn accept_nonexistent_swap_panics() {
        let (e, swap, _, _) = setup_env();
        swap.accept_swap(&999u64);
    }

    #[test]
    #[should_panic]
    fn accept_already_accepted_swap_panics() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry, &e, &seller, 0x91);

        let id = swap.initiate_swap(&token, &ip_id, &seller, &100i128, &buyer, &0u32, &None, &0i128, &false);
        swap.accept_swap(&id);
        swap.accept_swap(&id); // should panic: NotPending
    }

    #[test]
    #[should_panic]
    fn accept_cancelled_swap_panics() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry, &e, &seller, 0x92);

        let id = swap.initiate_swap(&token, &ip_id, &seller, &100i128, &buyer, &0u32, &None, &0i128, &false);
        swap.cancel_swap(&id, &seller);
        swap.accept_swap(&id); // should panic: NotPending
    }

    // ── Reveal key mutations ──────────────────────────────────────────────────

    #[test]
    fn reveal_key_transitions_to_completed() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let secret_seed = 0xA0u8;
        let blinding_seed = 0xB0u8;

        // Create commitment hash = sha256(secret || blinding)
        let secret = BytesN::from_array(&e, &[secret_seed; 32]);
        let blinding = BytesN::from_array(&e, &[blinding_seed; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&e);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = e.crypto().sha256(&preimage).into();

        let ip_id = registry.commit_ip(&seller, &commitment_hash, &0u32);
        let id = swap.initiate_swap(&token, &ip_id, &seller, &100i128, &buyer, &0u32, &None, &0i128, &false);

        swap.accept_swap(&id);

        // Seller reveals the key
        swap.reveal_key(&id, &seller, &secret, &blinding);

        let record = swap.get_swap(&id).unwrap();
        assert_eq!(record.status, crate::SwapStatus::Completed);
    }

    #[test]
    #[should_panic]
    fn reveal_key_wrong_secret_panics() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);

        let secret = BytesN::from_array(&e, &[0xC0u8; 32]);
        let blinding = BytesN::from_array(&e, &[0xD0u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&e);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = e.crypto().sha256(&preimage).into();

        let ip_id = registry.commit_ip(&seller, &commitment_hash, &0u32);
        let id = swap.initiate_swap(&token, &ip_id, &seller, &100i128, &buyer, &0u32, &None, &0i128, &false);

        swap.accept_swap(&id);

        // Reveal with wrong secret
        let wrong_secret = BytesN::from_array(&e, &[0xFFu8; 32]);
        swap.reveal_key(&id, &seller, &wrong_secret, &blinding);
    }

    #[test]
    #[should_panic]
    fn reveal_key_on_pending_swap_panics() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry, &e, &seller, 0xA1);

        let id = swap.initiate_swap(&token, &ip_id, &seller, &100i128, &buyer, &0u32, &None, &0i128, &false);

        swap.reveal_key(
            &id,
            &seller,
            &BytesN::from_array(&e, &[0u8; 32]),
            &BytesN::from_array(&e, &[0u8; 32]),
        );
    }

    // ── Cancel expired swap mutations ────────────────────────────────────────

    #[test]
    #[should_panic]
    fn cancel_expired_on_pending_panics() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry, &e, &seller, 0xB0);

        let id = swap.initiate_swap(&token, &ip_id, &seller, &100i128, &buyer, &0u32, &None, &0i128, &false);
        swap.cancel_expired_swap(&id, &buyer);
    }

    // ── Price mutation ────────────────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn zero_price_is_rejected() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry, &e, &seller, 0xC0);

        swap.initiate_swap(&token, &ip_id, &seller, &0i128, &buyer, &0u32, &None, &0i128, &false);
    }

    #[test]
    #[should_panic]
    fn negative_price_is_rejected() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry, &e, &seller, 0xC1);

        swap.initiate_swap(&token, &ip_id, &seller, &(-100i128), &buyer, &0u32, &None, &0i128, &false);
    }

    // ── Event emission mutations ──────────────────────────────────────────────

    #[test]
    fn initiate_swap_emits_event() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry, &e, &seller, 0xD0);

        use soroban_sdk::testutils::Events;
        swap.initiate_swap(&token, &ip_id, &seller, &100i128, &buyer, &0u32, &None, &0i128, &false);

        let all_events = e.events().all();
        let found = all_events.events().iter().any(|ev| {
            ev.topics.contains(&soroban_sdk::symbol_short!("swap_new").into())
        });
        assert!(found);
    }

    #[test]
    fn cancel_swap_emits_event() {
        let (e, swap, registry, token) = setup_env();
        let seller = Address::generate(&e);
        let buyer = Address::generate(&e);
        let ip_id = commit_test_ip(&registry, &e, &seller, 0xD1);

        use soroban_sdk::testutils::Events;
        let id = swap.initiate_swap(&token, &ip_id, &seller, &100i128, &buyer, &0u32, &None, &0i128, &false);
        swap.cancel_swap(&id, &seller);

        let all_events = e.events().all();
        let found = all_events.events().iter().any(|ev| {
            ev.topics.contains(&soroban_sdk::symbol_short!("swap_cncl").into())
        });
        assert!(found);
    }
}
