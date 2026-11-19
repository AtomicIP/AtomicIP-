// #1355: auction minimum increment and anti-sniping extension.
#[cfg(test)]
mod auction_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient};

    const START: u64 = 1_000_000;
    const DURATION: u64 = 1_000;
    const MIN_BID: i128 = 1_000;

    struct Fixture<'a> {
        env: &'a Env,
        client: AtomicSwapClient<'a>,
        token: Address,
        seller: Address,
        auction_id: u64,
    }

    fn setup(env: &Env, min_increment_bps: u32, anti_snipe_window_secs: u64) -> Fixture<'_> {
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = START);

        let admin = Address::generate(env);
        let seller = Address::generate(env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);
        let ip_id = registry.commit_ip(&seller, &BytesN::from_array(env, &[0x55u8; 32]));

        let token = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(&admin, &registry_id);

        let auction_id = client.start_ip_auction(
            &token,
            &ip_id,
            &seller,
            &MIN_BID,
            &DURATION,
            &min_increment_bps,
            &anti_snipe_window_secs,
        );

        Fixture {
            env,
            client,
            token,
            seller,
            auction_id,
        }
    }

    fn bid(f: &Fixture, amount: i128) {
        let bidder = Address::generate(f.env);
        StellarAssetClient::new(f.env, &f.token).mint(&bidder, &amount);
        f.client.place_bid(&f.auction_id, &bidder, &amount);
    }

    fn set_time(env: &Env, t: u64) {
        env.ledger().with_mut(|l| l.timestamp = t);
    }

    // ── Minimum increment ─────────────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn test_first_bid_below_min_bid_rejected() {
        let env = Env::default();
        let f = setup(&env, 500, 0);
        bid(&f, MIN_BID - 1);
    }

    #[test]
    #[should_panic]
    fn test_one_unit_raise_rejected_when_increment_set() {
        let env = Env::default();
        let f = setup(&env, 500, 0); // 5% minimum raise
        bid(&f, 1_000);
        bid(&f, 1_001);
    }

    #[test]
    #[should_panic]
    fn test_raise_below_increment_rejected() {
        let env = Env::default();
        let f = setup(&env, 500, 0);
        bid(&f, 1_000);
        bid(&f, 1_049); // needs 1_050
    }

    #[test]
    fn test_raise_at_exact_increment_accepted() {
        let env = Env::default();
        let f = setup(&env, 500, 0);
        bid(&f, 1_000);
        bid(&f, 1_050);
        assert_eq!(f.client.get_auction(&f.auction_id).highest_bid, 1_050);
    }

    #[test]
    fn test_zero_increment_still_requires_one_unit_raise() {
        let env = Env::default();
        let f = setup(&env, 0, 0);
        bid(&f, 1_000);
        bid(&f, 1_001);
        assert_eq!(f.client.get_auction(&f.auction_id).highest_bid, 1_001);
    }

    #[test]
    #[should_panic]
    fn test_increment_above_100_percent_rejected_at_start() {
        let env = Env::default();
        let _ = setup(&env, 10_001, 0);
    }

    // ── Anti-sniping extension ────────────────────────────────────────────────

    #[test]
    fn test_bid_inside_window_extends_end_time() {
        let env = Env::default();
        let f = setup(&env, 0, 300);
        set_time(&env, START + 800); // 200s left, inside the 300s window
        bid(&f, MIN_BID);
        assert_eq!(
            f.client.get_auction(&f.auction_id).end_time,
            START + 800 + 300
        );
    }

    #[test]
    fn test_bid_outside_window_does_not_extend_end_time() {
        let env = Env::default();
        let f = setup(&env, 0, 300);
        set_time(&env, START + 100); // 900s left, outside the window
        bid(&f, MIN_BID);
        assert_eq!(
            f.client.get_auction(&f.auction_id).end_time,
            START + DURATION
        );
    }

    #[test]
    fn test_zero_window_never_extends() {
        let env = Env::default();
        let f = setup(&env, 0, 0);
        set_time(&env, START + DURATION - 1);
        bid(&f, MIN_BID);
        assert_eq!(
            f.client.get_auction(&f.auction_id).end_time,
            START + DURATION
        );
    }

    #[test]
    fn test_bid_accepted_after_original_end_due_to_extension() {
        let env = Env::default();
        let f = setup(&env, 0, 300);
        set_time(&env, START + 800);
        bid(&f, MIN_BID); // end moves to START + 1100
        set_time(&env, START + DURATION + 50); // past the original end
        bid(&f, MIN_BID + 1);
        assert_eq!(f.client.get_auction(&f.auction_id).highest_bid, MIN_BID + 1);
    }

    #[test]
    fn test_seller_unchanged_by_bidding() {
        let env = Env::default();
        let f = setup(&env, 0, 300);
        set_time(&env, START + 800);
        bid(&f, MIN_BID);
        assert_eq!(f.client.get_auction(&f.auction_id).seller, f.seller);
    }
}
