//! #1354: IP auction bid guard tests — a seller must not bid on their own auction.

#[cfg(test)]
mod tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _, token::StellarAssetClient, Address, Bytes, BytesN, Env,
    };

    use crate::{
        AtomicSwap, AtomicSwapClient, MAX_AUCTION_DURATION_SECONDS, MIN_AUCTION_DURATION_SECONDS,
    };

    const MIN_BID: i128 = 100;
    const DURATION: u64 = MIN_AUCTION_DURATION_SECONDS;

    fn commit_ip(env: &Env, registry_id: &Address, owner: &Address) -> u64 {
        let registry = IpRegistryClient::new(env, registry_id);
        let secret = BytesN::from_array(env, &[1u8; 32]);
        let blinding = BytesN::from_array(env, &[2u8; 32]);
        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(secret));
        preimage.append(&Bytes::from(blinding));
        let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        registry.commit_ip(owner, &hash, &0u32)
    }

    struct Fixture {
        env: Env,
        contract_id: Address,
        token_id: Address,
        seller: Address,
        ip_id: u64,
    }

    impl Fixture {
        fn client(&self) -> AtomicSwapClient<'_> {
            AtomicSwapClient::new(&self.env, &self.contract_id)
        }
    }

    fn fixture() -> Fixture {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let seller = Address::generate(&env);
        let registry_id = env.register(IpRegistry, ());
        let ip_id = commit_ip(&env, &registry_id, &seller);

        let token_id = env
            .register_stellar_asset_contract_v2(deployer.clone())
            .address();
        StellarAssetClient::new(&env, &token_id).mint(&seller, &1_000_000);

        let contract_id = env.register(AtomicSwap, ());
        AtomicSwapClient::new(&env, &contract_id).initialize(&deployer, &registry_id);

        Fixture {
            env,
            contract_id,
            token_id,
            seller,
            ip_id,
        }
    }

    fn start(f: &Fixture, duration: u64) -> u64 {
        f.client()
            .start_ip_auction(&f.token_id, &f.ip_id, &f.seller, &MIN_BID, &duration)
    }

    #[test]
    fn test_auction_accepts_minimum_duration() {
        let f = fixture();
        let auction_id = start(&f, MIN_AUCTION_DURATION_SECONDS);
        let auction = f.client().get_auction(&auction_id);
        assert_eq!(
            auction.end_time - auction.start_time,
            MIN_AUCTION_DURATION_SECONDS
        );
    }

    #[test]
    fn test_auction_accepts_maximum_duration() {
        let f = fixture();
        let auction_id = start(&f, MAX_AUCTION_DURATION_SECONDS);
        let auction = f.client().get_auction(&auction_id);
        assert_eq!(
            auction.end_time - auction.start_time,
            MAX_AUCTION_DURATION_SECONDS
        );
    }

    #[test]
    #[should_panic]
    fn test_auction_rejects_zero_duration() {
        let f = fixture();
        start(&f, 0);
    }

    #[test]
    #[should_panic]
    fn test_auction_rejects_below_minimum_duration() {
        let f = fixture();
        start(&f, MIN_AUCTION_DURATION_SECONDS - 1);
    }

    #[test]
    #[should_panic]
    fn test_auction_rejects_above_maximum_duration() {
        let f = fixture();
        start(&f, MAX_AUCTION_DURATION_SECONDS + 1);
    }

    #[test]
    #[should_panic]
    fn test_auction_rejects_u64_max_duration() {
        let f = fixture();
        start(&f, u64::MAX);
    }

    #[test]
    #[should_panic]
    fn test_seller_cannot_bid_on_own_auction() {
        let f = fixture();
        let client = f.client();
        let auction_id =
            client.start_ip_auction(&f.token_id, &f.ip_id, &f.seller, &MIN_BID, &DURATION);

        // #1354: the seller bidding on their own auction must panic.
        client.place_bid(&auction_id, &f.seller, &(MIN_BID + 1));
    }
}
