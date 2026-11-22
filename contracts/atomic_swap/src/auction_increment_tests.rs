// #1355: auction minimum increment and anti-sniping extension.
#[cfg(test)]
mod auction_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::{StellarAssetClient, TokenClient},
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
        registry_id: Address,
        ip_id: u64,
    }

    fn setup(env: &Env, min_increment_bps: u32, anti_snipe_window_secs: u64) -> Fixture<'_> {
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = START);

        let admin = Address::generate(env);
        let seller = Address::generate(env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);
        let ip_id = registry.commit_ip(&seller, &BytesN::from_array(env, &[0x55u8; 32]), &0u32);

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
            registry_id,
            ip_id,
        }
    }

    fn bid(f: &Fixture, amount: i128) {
        let bidder = Address::generate(f.env);
        StellarAssetClient::new(f.env, &f.token).mint(&bidder, &amount);
        f.client.place_bid(&f.auction_id, &bidder, &amount);
    }

    /// Mint `amount` to a fresh bidder and bid as them. Returns the bidder.
    fn bid_as_new(f: &Fixture, amount: i128) -> Address {
        let bidder = Address::generate(f.env);
        StellarAssetClient::new(f.env, &f.token).mint(&bidder, &amount);
        f.client.place_bid(&f.auction_id, &bidder, &amount);
        bidder
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

    // ── Pull-based refunds (#1356) ────────────────────────────────────────────

    #[test]
    fn test_outbid_refund_is_credited_not_pushed() {
        let env = Env::default();
        let f = setup(&env, 0, 0);
        let first = bid_as_new(&f, 1_000);
        bid_as_new(&f, 2_000);

        // The previous bidder's balance is untouched until they withdraw.
        assert_eq!(TokenClient::new(&env, &f.token).balance(&first), 0);
        assert_eq!(f.client.get_pending_refund(&first, &f.token), 1_000);
    }

    #[test]
    fn test_new_bid_succeeds_when_outbid_bidder_cannot_receive() {
        let env = Env::default();
        let f = setup(&env, 0, 0);
        let first = bid_as_new(&f, 1_000);

        // Deauthorizing the previous bidder makes any transfer to them fail.
        // The old inline refund would have made this outbid revert.
        StellarAssetClient::new(&env, &f.token).set_authorized(&first, &false);

        bid_as_new(&f, 2_000);
        assert_eq!(f.client.get_auction(&f.auction_id).highest_bid, 2_000);
        assert_eq!(f.client.get_pending_refund(&first, &f.token), 1_000);
    }

    #[test]
    fn test_withdraw_refund_pays_outbid_bidder() {
        let env = Env::default();
        let f = setup(&env, 0, 0);
        let first = bid_as_new(&f, 1_000);
        bid_as_new(&f, 2_000);

        let paid = f.client.withdraw_refund(&first, &f.token);

        assert_eq!(paid, 1_000);
        assert_eq!(TokenClient::new(&env, &f.token).balance(&first), 1_000);
        assert_eq!(f.client.get_pending_refund(&first, &f.token), 0);
    }

    #[test]
    #[should_panic]
    fn test_withdraw_refund_twice_rejected() {
        let env = Env::default();
        let f = setup(&env, 0, 0);
        let first = bid_as_new(&f, 1_000);
        bid_as_new(&f, 2_000);

        f.client.withdraw_refund(&first, &f.token);
        f.client.withdraw_refund(&first, &f.token);
    }

    #[test]
    #[should_panic]
    fn test_withdraw_refund_with_nothing_owed_rejected() {
        let env = Env::default();
        let f = setup(&env, 0, 0);
        let stranger = Address::generate(&env);
        f.client.withdraw_refund(&stranger, &f.token);
    }

    // ── Seller ownership re-check at finalization (#1357) ─────────────────────

    #[test]
    fn test_finalize_with_seller_still_owner_creates_swap() {
        let env = Env::default();
        let f = setup(&env, 0, 0);
        let winner = bid_as_new(&f, 1_000);
        set_time(&env, START + DURATION);

        f.client.finalize_auction(&f.auction_id);

        let swap = f.client.get_swap(&0).expect("swap created");
        assert_eq!(swap.buyer, winner);
        assert_eq!(swap.price, 1_000);
        assert!(!f.client.get_auction(&f.auction_id).failed);
    }

    #[test]
    fn test_transfer_during_auction_refunds_winner() {
        let env = Env::default();
        let f = setup(&env, 0, 0);
        let winner = bid_as_new(&f, 1_000);

        let registry = IpRegistryClient::new(&env, &f.registry_id);
        registry.transfer_ip(&f.ip_id, &Address::generate(&env));

        set_time(&env, START + DURATION);
        f.client.finalize_auction(&f.auction_id);

        assert!(f.client.get_swap(&0).is_none());
        assert!(f.client.get_auction(&f.auction_id).failed);
        assert_eq!(f.client.get_pending_refund(&winner, &f.token), 1_000);

        let paid = f.client.withdraw_refund(&winner, &f.token);
        assert_eq!(paid, 1_000);
        assert_eq!(TokenClient::new(&env, &f.token).balance(&winner), 1_000);
    }

    #[test]
    fn test_revoke_during_auction_refunds_winner() {
        let env = Env::default();
        let f = setup(&env, 0, 0);
        let winner = bid_as_new(&f, 1_000);

        let registry = IpRegistryClient::new(&env, &f.registry_id);
        registry.revoke_ip(&f.ip_id);

        set_time(&env, START + DURATION);
        f.client.finalize_auction(&f.auction_id);

        assert!(f.client.get_swap(&0).is_none());
        assert!(f.client.get_auction(&f.auction_id).failed);
        assert_eq!(f.client.get_pending_refund(&winner, &f.token), 1_000);
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
