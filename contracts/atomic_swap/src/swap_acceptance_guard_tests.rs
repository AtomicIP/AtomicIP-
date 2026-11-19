//! Regression tests for buyer-side acceptance guards on `Pending` swaps.
//!
//! - #1335: `accept_swap` and `batch_accept_swaps` must reject swaps whose
//!   expiry has been reached.
//! - #1336: `escrow_deposit` must reject deposits after expiry.
//! - #1337: `batch_accept_swaps` must reject duplicate swap IDs.
//! - #1338: `batch_accept_swaps` must apply the same reputation and
//!   condition gates as `accept_swap`.

#[cfg(test)]
mod tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::{StellarAssetClient, TokenClient},
        Address, Bytes, BytesN, Env, Vec,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapCondition, SwapStatus};

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Commits an IP owned by `owner` and returns its id.
    fn commit_ip(env: &Env, registry_id: &Address, owner: &Address, seed: u8) -> u64 {
        let registry = IpRegistryClient::new(env, registry_id);
        let secret = BytesN::from_array(env, &[seed; 32]);
        let blinding = BytesN::from_array(env, &[seed.wrapping_add(1); 32]);
        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(secret));
        preimage.append(&Bytes::from(blinding));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        registry.commit_ip(owner, &commitment_hash, &0u32)
    }

    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    /// Deploys the swap contract wired to a fresh registry and returns
    /// `(swap_client, registry_id)`.
    fn setup_swap_contract(env: &Env) -> (AtomicSwapClient<'_>, Address) {
        let deployer = Address::generate(env);
        let registry_id = env.register(IpRegistry, ());
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(&deployer, &registry_id);
        (client, registry_id)
    }

    fn set_timestamp(env: &Env, ts: u64) {
        env.ledger().with_mut(|li| li.timestamp = ts);
    }

    /// Writes buyer-set conditions onto a Pending swap record. The public API only
    /// attaches conditions while accepting, so tests set them directly to exercise
    /// the batch path against a Pending swap.
    fn attach_conditions(
        env: &Env,
        contract_id: &Address,
        swap_id: u64,
        conditions: Vec<SwapCondition>,
    ) {
        env.as_contract(contract_id, || {
            let mut swap = crate::swap::load_swap(env, swap_id);
            swap.conditions = conditions;
            crate::swap::save_swap(env, swap_id, &swap);
        });
    }

    // ── #1335: accept_swap / batch_accept_swaps honour swap expiry ────────────

    /// One ledger tick before expiry the buyer can still accept.
    #[test]
    fn test_accept_swap_one_second_before_expiry_succeeds() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (client, registry_id) = setup_swap_contract(&env);
        let ip_id = commit_ip(&env, &registry_id, &seller, 0x11);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );
        let expiry = client.get_swap(&swap_id).unwrap().expiry;

        set_timestamp(&env, expiry - 1);
        client.accept_swap(&swap_id);

        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Accepted
        );
    }

    /// At exactly the expiry timestamp the swap is dead and acceptance fails
    /// with SwapExpired (#81).
    #[test]
    #[should_panic(expected = "Error(Contract, #81)")]
    fn test_accept_swap_at_expiry_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (client, registry_id) = setup_swap_contract(&env);
        let ip_id = commit_ip(&env, &registry_id, &seller, 0x12);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );
        let expiry = client.get_swap(&swap_id).unwrap().expiry;

        set_timestamp(&env, expiry);
        client.accept_swap(&swap_id);
    }

    /// A batch containing one expired swap panics as a whole, so the live swap
    /// listed alongside it is not accepted either.
    #[test]
    #[should_panic(expected = "Error(Contract, #81)")]
    fn test_batch_accept_with_one_expired_swap_reverts_whole_batch() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (client, registry_id) = setup_swap_contract(&env);
        let ip_a = commit_ip(&env, &registry_id, &seller, 0x13);
        let ip_b = commit_ip(&env, &registry_id, &seller, 0x15);
        let token_id = setup_token(&env, &admin, &buyer, 2000);

        // Swap A is created first so it expires first; swap B is created later.
        set_timestamp(&env, 1_000);
        let swap_a = client.initiate_swap(
            &token_id, &ip_a, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );
        let expiry_a = client.get_swap(&swap_a).unwrap().expiry;

        set_timestamp(&env, expiry_a - 1);
        let swap_b = client.initiate_swap(
            &token_id, &ip_b, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );
        assert!(client.get_swap(&swap_b).unwrap().expiry > expiry_a);

        // Now at expiry_a: A is expired, B is still live.
        set_timestamp(&env, expiry_a);
        let mut ids = Vec::new(&env);
        ids.push_back(swap_b);
        ids.push_back(swap_a);
        client.batch_accept_swaps(&ids, &buyer);
    }

    // ── #1336: escrow_deposit honours swap expiry ─────────────────────────────

    /// Depositing into escrow after expiry fails and the buyer's balance is
    /// unchanged, because the guard runs before the token transfer.
    #[test]
    fn test_escrow_deposit_after_expiry_fails_and_balance_unchanged() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (client, registry_id) = setup_swap_contract(&env);
        let ip_id = commit_ip(&env, &registry_id, &seller, 0x21);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let token = TokenClient::new(&env, &token_id);

        set_timestamp(&env, 1_000);
        let timeout = 1_000 + 3600;
        let swap_id =
            client.initiate_escrow_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &timeout);

        set_timestamp(&env, timeout);
        let result = client.try_escrow_deposit(&swap_id);

        assert!(result.is_err());
        assert_eq!(token.balance(&buyer), 1000);
        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Pending
        );
    }

    // ── #1337: batch_accept_swaps rejects duplicate IDs ───────────────────────

    /// `[id, id]` for a Pending swap is rejected and no tokens move.
    #[test]
    fn test_batch_accept_duplicate_id_rejected_no_tokens_move() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (client, registry_id) = setup_swap_contract(&env);
        let ip_id = commit_ip(&env, &registry_id, &seller, 0x31);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let token = TokenClient::new(&env, &token_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        let mut ids = Vec::new(&env);
        ids.push_back(swap_id);
        ids.push_back(swap_id);
        let result = client.try_batch_accept_swaps(&ids, &buyer);

        assert!(result.is_err());
        assert_eq!(token.balance(&buyer), 1000);
        assert_eq!(token.balance(&client.address), 0);
        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Pending
        );
    }

    // ── #1338: batch_accept_swaps enforces the same gates as accept_swap ──────

    /// A buyer below the seller's minimum reputation cannot accept through the
    /// batch path, and no tokens move.
    #[test]
    fn test_batch_accept_rejected_below_seller_min_reputation() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (client, registry_id) = setup_swap_contract(&env);
        let ip_id = commit_ip(&env, &registry_id, &seller, 0x41);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let token = TokenClient::new(&env, &token_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );
        // Default buyer reputation is 50; require 60.
        client.set_reputation_multiplier(&swap_id, &60_u32);

        let mut ids = Vec::new(&env);
        ids.push_back(swap_id);
        let result = client.try_batch_accept_swaps(&ids, &buyer);

        assert!(result.is_err());
        assert_eq!(token.balance(&buyer), 1000);
        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Pending
        );
    }

    /// A `PriceBelow` condition that fails blocks batch acceptance. The swap
    /// price is 500, so a threshold of 100 is not met.
    #[test]
    fn test_batch_accept_rejected_when_price_below_condition_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (client, registry_id) = setup_swap_contract(&env);
        let ip_id = commit_ip(&env, &registry_id, &seller, 0x42);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let token = TokenClient::new(&env, &token_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );
        attach_conditions(
            &env,
            &client.address,
            swap_id,
            soroban_sdk::vec![&env, SwapCondition::PriceBelow(100)],
        );

        let mut ids = Vec::new(&env);
        ids.push_back(swap_id);
        let result = client.try_batch_accept_swaps(&ids, &buyer);

        assert!(result.is_err());
        assert_eq!(token.balance(&buyer), 1000);
        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Pending
        );
    }

    /// A `TimeAfter` condition that is not yet satisfied blocks batch
    /// acceptance, and no tokens move.
    #[test]
    fn test_batch_accept_rejected_when_time_after_condition_fails() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (client, registry_id) = setup_swap_contract(&env);
        let ip_id = commit_ip(&env, &registry_id, &seller, 0x44);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let token = TokenClient::new(&env, &token_id);

        set_timestamp(&env, 1_000);
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );
        attach_conditions(
            &env,
            &client.address,
            swap_id,
            soroban_sdk::vec![&env, SwapCondition::TimeAfter(5_000)],
        );

        let mut ids = Vec::new(&env);
        ids.push_back(swap_id);
        let result = client.try_batch_accept_swaps(&ids, &buyer);

        assert!(result.is_err());
        assert_eq!(token.balance(&buyer), 1000);
        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Pending
        );
    }
}
