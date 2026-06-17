/// #550 Chaos Engineering Tests — Atomic Swap
///
/// Tests system resilience by injecting faults: concurrent operations,
/// unexpected state transitions, resource exhaustion, and partial failures.
///
/// Run with: cargo test chaos_ -p atomic_swap
#[cfg(test)]
mod chaos_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Bytes, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_commitment(env: &Env, secret: &BytesN<32>, blinding: &BytesN<32>) -> BytesN<32> {
        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(secret.clone()));
        preimage.append(&Bytes::from(blinding.clone()));
        env.crypto().sha256(&preimage).into()
    }

    struct TestContext {
        env: Env,
        swap: AtomicSwapClient<'static>,
        registry: IpRegistryClient<'static>,
        token: Address,
    }

    fn setup(price: i128) -> (TestContext, u64, BytesN<32>, BytesN<32>, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);

        let secret = BytesN::from_array(&env, &[0xABu8; 32]);
        let blinding = BytesN::from_array(&env, &[0xCDu8; 32]);
        let hash = make_commitment(&env, &secret, &blinding);
        let ip_id = registry.commit_ip(&seller, &hash, &0u32);

        let token = env.register_stellar_asset_contract_v2(admin).address();
        StellarAssetClient::new(&env, &token).mint(&buyer, &price);

        let swap_id = env.register(AtomicSwap, ());
        let swap = AtomicSwapClient::new(&env, &swap_id);
        swap.initialize(&registry_id);

        let ctx = TestContext { env, swap, registry, token };
        (ctx, ip_id, secret, blinding, seller, buyer)
    }

    // ── Fault: double-accept ──────────────────────────────────────────────────

    /// Chaos: accepting the same swap twice must be idempotent-safe (second panics).
    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn chaos_double_accept_rejected() {
        let (ctx, ip_id, _secret, _blinding, seller, buyer) = setup(1000);
        let swap_id = ctx.swap.initiate_swap(
            &ctx.token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );
        ctx.swap.accept_swap(&swap_id);
        ctx.swap.accept_swap(&swap_id); // must panic
    }

    // ── Fault: reveal before accept ───────────────────────────────────────────

    /// Chaos: revealing the key before the buyer accepts must be rejected.
    #[test]
    #[should_panic(expected = "Error(Contract, #8)")]
    fn chaos_reveal_before_accept_rejected() {
        let (ctx, ip_id, secret, blinding, seller, buyer) = setup(1000);
        let swap_id = ctx.swap.initiate_swap(
            &ctx.token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );
        ctx.swap.reveal_key(&swap_id, &seller, &secret, &blinding); // must panic
    }

    // ── Fault: cancel after completion ───────────────────────────────────────

    /// Chaos: cancelling a completed swap must be rejected.
    #[test]
    #[should_panic]
    fn chaos_cancel_after_completion_rejected() {
        let (ctx, ip_id, secret, blinding, seller, buyer) = setup(1000);
        let swap_id = ctx.swap.initiate_swap(
            &ctx.token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );
        ctx.swap.accept_swap(&swap_id);
        ctx.swap.reveal_key(&swap_id, &seller, &secret, &blinding);
        ctx.swap.cancel_swap(&swap_id, &seller); // must panic
    }

    // ── Fault: wrong key on accepted swap ────────────────────────────────────

    /// Chaos: providing a wrong decryption key must be rejected and leave swap Accepted.
    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn chaos_wrong_key_rejected() {
        let (ctx, ip_id, _secret, blinding, seller, buyer) = setup(1000);
        let swap_id = ctx.swap.initiate_swap(
            &ctx.token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );
        ctx.swap.accept_swap(&swap_id);
        let wrong = BytesN::from_array(&ctx.env, &[0xFFu8; 32]);
        ctx.swap.reveal_key(&swap_id, &seller, &wrong, &blinding); // must panic
    }

    // ── Fault: zero-price swap ────────────────────────────────────────────────

    /// Chaos: initiating a swap with price=0 must be rejected.
    #[test]
    #[should_panic]
    fn chaos_zero_price_rejected() {
        let (ctx, ip_id, _secret, _blinding, seller, buyer) = setup(0);
        ctx.swap.initiate_swap(
            &ctx.token, &ip_id, &seller, &0, &buyer, &0_u32, &None, &0i128, &false,
        );
    }

    // ── Fault: cancel then accept ─────────────────────────────────────────────

    /// Chaos: accepting a cancelled swap must be rejected.
    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn chaos_accept_after_cancel_rejected() {
        let (ctx, ip_id, _secret, _blinding, seller, buyer) = setup(1000);
        let swap_id = ctx.swap.initiate_swap(
            &ctx.token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );
        ctx.swap.cancel_swap(&swap_id, &seller);
        ctx.swap.accept_swap(&swap_id); // must panic
    }

    // ── Fault: reveal with wrong seller ──────────────────────────────────────

    /// Chaos: a third party cannot reveal the key on behalf of the seller.
    #[test]
    #[should_panic]
    fn chaos_reveal_by_non_seller_rejected() {
        let (ctx, ip_id, secret, blinding, seller, buyer) = setup(1000);
        let swap_id = ctx.swap.initiate_swap(
            &ctx.token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );
        ctx.swap.accept_swap(&swap_id);
        let impostor = Address::generate(&ctx.env);
        ctx.swap.reveal_key(&swap_id, &impostor, &secret, &blinding); // must panic
    }

    // ── Fault: multiple swaps for same IP ────────────────────────────────────

    /// Chaos: only one active swap per IP is allowed; a second initiation must fail.
    #[test]
    #[should_panic]
    fn chaos_duplicate_active_swap_rejected() {
        let (ctx, ip_id, _secret, _blinding, seller, buyer) = setup(2000);
        StellarAssetClient::new(&ctx.env, &ctx.token).mint(&buyer, &2000);
        ctx.swap.initiate_swap(
            &ctx.token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );
        // Second swap for the same IP while first is still Pending must fail.
        ctx.swap.initiate_swap(
            &ctx.token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );
    }

    // ── Fault: ledger time jump ───────────────────────────────────────────────

    /// Chaos: after a large ledger advance the swap state must remain consistent.
    #[test]
    fn chaos_state_consistent_after_time_jump() {
        let (ctx, ip_id, _secret, _blinding, seller, buyer) = setup(1000);
        let swap_id = ctx.swap.initiate_swap(
            &ctx.token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );

        // Simulate a large ledger jump (e.g. network halt).
        ctx.env.ledger().with_mut(|l| {
            l.sequence_number += 1_000_000;
            l.timestamp += 86_400 * 30; // 30 days
        });

        // Swap must still be retrievable and in Pending state.
        let swap = ctx.swap.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Pending);
    }

    // ── Fault: sequential state machine exhaustion ────────────────────────────

    /// Chaos: run the full happy path N times to verify no state leaks between swaps.
    #[test]
    fn chaos_repeated_full_lifecycle() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let token = env.register_stellar_asset_contract_v2(admin).address();

        let swap_id_contract = env.register(AtomicSwap, ());
        let swap = AtomicSwapClient::new(&env, &swap_id_contract);
        swap.initialize(&registry_id);

        for i in 1u8..=5 {
            let seller = Address::generate(&env);
            let buyer = Address::generate(&env);
            let secret = BytesN::from_array(&env, &[i; 32]);
            let blinding = BytesN::from_array(&env, &[i.wrapping_add(0x80); 32]);
            let hash = make_commitment(&env, &secret, &blinding);
            let ip_id = registry.commit_ip(&seller, &hash, &0u32);

            StellarAssetClient::new(&env, &token).mint(&buyer, &1000);

            let swap_id = swap.initiate_swap(
                &token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
            );
            swap.accept_swap(&swap_id);
            swap.reveal_key(&swap_id, &seller, &secret, &blinding);

            assert_eq!(swap.get_swap(&swap_id).unwrap().status, SwapStatus::Completed);
        }
    }
}
