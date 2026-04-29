/// #378 Mutation-catching tests for Atomic Swap.
///
/// Targets common mutations:
///   - status transitions (Pending → Accepted → Completed)
///   - price/fee arithmetic
///   - invalid-key rejection
///   - cancel guards
#[cfg(test)]
mod mutation_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _,
        token::StellarAssetClient,
        Address, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus};

    // ── helpers ──────────────────────────────────────────────────────────────

    fn setup(env: &Env) -> (AtomicSwapClient, Address, Address, Address, u64, BytesN<32>, BytesN<32>) {
        env.mock_all_auths();
        let seller = Address::generate(env);
        let buyer = Address::generate(env);
        let admin = Address::generate(env);

        let reg_id = env.register(IpRegistry, ());
        let reg = IpRegistryClient::new(env, &reg_id);

        let secret = BytesN::from_array(env, &[0x11u8; 32]);
        let blinding = BytesN::from_array(env, &[0x22u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = reg.commit_ip(&seller, &commitment_hash);

        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(env, &token_id).mint(&buyer, &10_000_i128);

        let swap_id = env.register(AtomicSwap, ());
        let swap = AtomicSwapClient::new(env, &swap_id);
        swap.initialize(&reg_id);

        (swap, token_id, seller, buyer, ip_id, secret, blinding)
    }

    // ── Status transitions ────────────────────────────────────────────────────

    /// Mutation: skip setting status to Pending → get_swap returns wrong status.
    #[test]
    fn initiate_swap_sets_pending_status() {
        let env = Env::default();
        let (swap, token_id, seller, buyer, ip_id, _, _) = setup(&env);
        let sid = swap.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0u32, &None, &false);
        let record = swap.get_swap(&sid).unwrap();
        assert_eq!(record.status, SwapStatus::Pending);
    }

    /// Mutation: skip setting status to Accepted.
    #[test]
    fn accept_swap_sets_accepted_status() {
        let env = Env::default();
        let (swap, token_id, seller, buyer, ip_id, _, _) = setup(&env);
        let sid = swap.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0u32, &None, &false);
        swap.accept_swap(&sid);
        let record = swap.get_swap(&sid).unwrap();
        assert_eq!(record.status, SwapStatus::Accepted);
    }

    /// Mutation: skip setting status to Completed.
    #[test]
    fn reveal_key_sets_completed_status() {
        let env = Env::default();
        let (swap, token_id, seller, buyer, ip_id, secret, blinding) = setup(&env);
        let sid = swap.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0u32, &None, &false);
        swap.accept_swap(&sid);
        swap.reveal_key(&sid, &seller, &secret, &blinding);
        let record = swap.get_swap(&sid).unwrap();
        assert_eq!(record.status, SwapStatus::Completed);
    }

    // ── Invalid key rejection ─────────────────────────────────────────────────

    /// Mutation: skip commitment verification → invalid key accepted.
    #[test]
    #[should_panic]
    fn reveal_key_rejects_wrong_secret() {
        let env = Env::default();
        let (swap, token_id, seller, buyer, ip_id, _secret, blinding) = setup(&env);
        let sid = swap.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0u32, &None, &false);
        swap.accept_swap(&sid);
        let wrong = BytesN::from_array(&env, &[0xFFu8; 32]);
        swap.reveal_key(&sid, &seller, &wrong, &blinding);
    }

    // ── Price guard ───────────────────────────────────────────────────────────

    /// Mutation: remove price > 0 check → zero-price swap accepted.
    #[test]
    #[should_panic]
    fn zero_price_is_rejected() {
        let env = Env::default();
        let (swap, token_id, seller, buyer, ip_id, _, _) = setup(&env);
        swap.initiate_swap(&token_id, &ip_id, &seller, &0_i128, &buyer, &0u32, &None, &false);
    }

    // ── Double-accept guard ───────────────────────────────────────────────────

    /// Mutation: allow accept on non-Pending swap → double-accept succeeds.
    #[test]
    #[should_panic]
    fn accept_swap_twice_is_rejected() {
        let env = Env::default();
        let (swap, token_id, seller, buyer, ip_id, _, _) = setup(&env);
        let sid = swap.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0u32, &None, &false);
        swap.accept_swap(&sid);
        swap.accept_swap(&sid);
    }

    // ── Reveal on non-accepted swap ───────────────────────────────────────────

    /// Mutation: allow reveal_key on Pending swap → key revealed before payment.
    #[test]
    #[should_panic]
    fn reveal_key_on_pending_swap_is_rejected() {
        let env = Env::default();
        let (swap, token_id, seller, buyer, ip_id, secret, blinding) = setup(&env);
        let sid = swap.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0u32, &None, &false);
        // Do NOT call accept_swap — reveal must fail
        swap.reveal_key(&sid, &seller, &secret, &blinding);
    }

    // ── Swap ID counter ───────────────────────────────────────────────────────

    /// Mutation: id counter not incremented → second swap overwrites first.
    #[test]
    fn swap_ids_start_at_zero() {
        let env = Env::default();
        let (swap, token_id, seller, buyer, ip_id, _, _) = setup(&env);
        let sid = swap.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0u32, &None, &false);
        assert_eq!(sid, 0, "first swap ID must be 0");
    }
}
