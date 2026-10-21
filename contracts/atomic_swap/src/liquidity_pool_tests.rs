//! #1088: Liquidity pool integration tests — constant-product pricing, slippage
//! protection, reserve tracking and admin pool creation.

#[cfg(test)]
mod tests {
    use ip_registry::IpRegistry;
    use soroban_sdk::{
        testutils::Address as _,
        token::{Client as TokenClient, StellarAssetClient},
        Address, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient};

    const COMMITMENT: u64 = 7;
    const IP_RESERVE: u64 = 1_000;
    const TOKEN_RESERVE: i128 = 10_000;
    const MAX_SLIPPAGE_BPS: u32 = 1_000;

    struct Fixture {
        env: Env,
        contract_id: Address,
        token_id: Address,
        admin: Address,
        buyer: Address,
    }

    impl Fixture {
        fn client(&self) -> AtomicSwapClient<'_> {
            AtomicSwapClient::new(&self.env, &self.contract_id)
        }

        fn token(&self) -> TokenClient<'_> {
            TokenClient::new(&self.env, &self.token_id)
        }
    }

    /// Creates a 1000-IP / 10000-token pool with a 10% slippage cap.
    fn fixture_with_pool() -> Fixture {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let buyer = Address::generate(&env);
        let registry_id = env.register(IpRegistry, ());

        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(&env, &token_id).mint(&admin, &TOKEN_RESERVE);
        StellarAssetClient::new(&env, &token_id).mint(&buyer, &1_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);
        client.set_admin(&admin);
        client.create_liquidity_pool(
            &admin,
            &COMMITMENT,
            &token_id,
            &IP_RESERVE,
            &TOKEN_RESERVE,
            &MAX_SLIPPAGE_BPS,
        );

        Fixture {
            env,
            contract_id,
            token_id,
            admin,
            buyer,
        }
    }

    #[test]
    fn test_create_pool_funds_token_reserve_and_stores_state() {
        let f = fixture_with_pool();
        let pool = f.client().get_liquidity_pool(&COMMITMENT).unwrap();

        assert_eq!(pool.ip_reserve, IP_RESERVE);
        assert_eq!(pool.token_reserve, TOKEN_RESERVE);
        assert_eq!(pool.max_slippage_bps, MAX_SLIPPAGE_BPS);
        assert_eq!(pool.token, f.token_id);
        assert_eq!(f.token().balance(&f.contract_id), TOKEN_RESERVE);
        assert_eq!(f.client().get_pool_price(&COMMITMENT), 10_000_000);
    }

    #[test]
    fn test_small_swap_within_slippage_executes_and_updates_reserves() {
        let f = fixture_with_pool();

        // x·y = k: out = 1000 * 100 / (10000 + 100) = 9 (floored); spot would be 10,
        // so the impact is 1000 bps, exactly at the cap and therefore allowed.
        assert!(f.client().swap_via_pool(&f.buyer, &COMMITMENT, &100));

        let pool = f.client().get_liquidity_pool(&COMMITMENT).unwrap();
        assert_eq!(pool.ip_reserve, 991);
        assert_eq!(pool.token_reserve, TOKEN_RESERVE + 100);
        assert_eq!(f.client().get_pool_holdings(&COMMITMENT, &f.buyer), 9);
        assert_eq!(f.token().balance(&f.contract_id), TOKEN_RESERVE + 100);
    }

    #[test]
    fn test_swap_exceeding_slippage_returns_false_without_state_change() {
        let f = fixture_with_pool();
        let before = f.client().get_liquidity_pool(&COMMITMENT).unwrap();
        let buyer_before = f.token().balance(&f.buyer);

        // 2000 tokens: out = 166 against a spot value of 200, ~17% impact, above the 10% cap.
        assert!(!f.client().swap_via_pool(&f.buyer, &COMMITMENT, &2_000));

        assert_eq!(f.client().get_liquidity_pool(&COMMITMENT).unwrap(), before);
        assert_eq!(f.client().get_pool_holdings(&COMMITMENT, &f.buyer), 0);
        assert_eq!(f.token().balance(&f.buyer), buyer_before);
    }

    #[test]
    fn test_zero_amount_returns_false() {
        let f = fixture_with_pool();
        assert!(!f.client().swap_via_pool(&f.buyer, &COMMITMENT, &0));
    }

    #[test]
    fn test_pool_price_moves_up_after_buys() {
        let f = fixture_with_pool();
        let before = f.client().get_pool_price(&COMMITMENT);
        assert!(f.client().swap_via_pool(&f.buyer, &COMMITMENT, &100));
        assert!(f.client().get_pool_price(&COMMITMENT) > before);
    }

    #[test]
    fn test_consecutive_swaps_accumulate_holdings() {
        let f = fixture_with_pool();
        assert!(f.client().swap_via_pool(&f.buyer, &COMMITMENT, &100));
        let first = f.client().get_pool_holdings(&COMMITMENT, &f.buyer);
        assert!(f.client().swap_via_pool(&f.buyer, &COMMITMENT, &100));
        assert!(f.client().get_pool_holdings(&COMMITMENT, &f.buyer) > first);
    }

    #[test]
    #[should_panic]
    fn test_swap_on_missing_pool_panics() {
        let f = fixture_with_pool();
        f.client().swap_via_pool(&f.buyer, &999, &10);
    }

    #[test]
    #[should_panic]
    fn test_duplicate_pool_panics() {
        let f = fixture_with_pool();
        f.client().create_liquidity_pool(
            &f.admin,
            &COMMITMENT,
            &f.token_id,
            &IP_RESERVE,
            &TOKEN_RESERVE,
            &MAX_SLIPPAGE_BPS,
        );
    }

    #[test]
    #[should_panic]
    fn test_pool_with_zero_ip_reserve_panics() {
        let f = fixture_with_pool();
        f.client().create_liquidity_pool(
            &f.admin,
            &8,
            &f.token_id,
            &0,
            &TOKEN_RESERVE,
            &MAX_SLIPPAGE_BPS,
        );
    }

    #[test]
    #[should_panic]
    fn test_pool_slippage_above_100_percent_panics() {
        let f = fixture_with_pool();
        f.client().create_liquidity_pool(
            &f.admin,
            &8,
            &f.token_id,
            &IP_RESERVE,
            &TOKEN_RESERVE,
            &10_001,
        );
    }

    #[test]
    #[should_panic]
    fn test_create_pool_by_non_admin_panics() {
        let f = fixture_with_pool();
        let outsider = Address::generate(&f.env);
        f.client().create_liquidity_pool(
            &outsider,
            &8,
            &f.token_id,
            &IP_RESERVE,
            &TOKEN_RESERVE,
            &MAX_SLIPPAGE_BPS,
        );
    }
}
