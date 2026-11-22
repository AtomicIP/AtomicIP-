//! #1347: The self-trade guard (`buyer != seller`, `SelfTradeNotAllowed` = 79)
//! must cover every swap-creation path, not only `initiate_swap` and
//! `batch_initiate_swap`.

#[cfg(test)]
mod self_trade_guard_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        token::StellarAssetClient,
        Address, BytesN, Env, Vec,
    };

    use crate::{AtomicSwap, AtomicSwapClient, PaymentSchedule};

    const SELF_TRADE_NOT_ALLOWED: u32 = 79;
    const PRICE: i128 = 1_000;

    fn self_trade_error() -> soroban_sdk::Error {
        soroban_sdk::Error::from_contract_error(SELF_TRADE_NOT_ALLOWED)
    }

    struct Fixture {
        env: Env,
        contract_id: Address,
        seller: Address,
        other_buyer: Address,
        token: Address,
        ip_id: u64,
    }

    impl Fixture {
        fn client(&self) -> AtomicSwapClient<'_> {
            AtomicSwapClient::new(&self.env, &self.contract_id)
        }
    }

    /// Registers a seller-owned IP, a token, and an initialized AtomicSwap contract.
    fn setup() -> Fixture {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let seller = Address::generate(&env);
        let other_buyer = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let hash = BytesN::from_array(&env, &[0x47; 32]);
        let ip_id = IpRegistryClient::new(&env, &registry_id).commit_ip(&seller, &hash, &0u32);

        let token = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(&env, &token).mint(&other_buyer, &1_000_000);

        let contract_id = env.register(AtomicSwap, ());
        AtomicSwapClient::new(&env, &contract_id).initialize(&admin, &registry_id);

        Fixture {
            env,
            contract_id,
            seller,
            other_buyer,
            token,
            ip_id,
        }
    }

    // ── Control: a distinct buyer is accepted, so the fixture itself is valid ──

    #[test]
    fn test_initiate_swap_installment_allows_distinct_buyer() {
        let f = setup();
        let res = f.client().try_initiate_swap_installment(
            &f.token,
            &f.ip_id,
            &f.seller,
            &PRICE,
            &f.other_buyer,
            &2,
        );
        assert!(res.is_ok());
    }

    // ── One self-trade test per previously unguarded entry point ──────────────

    #[test]
    fn test_initiate_swap_installment_rejects_self_trade() {
        let f = setup();
        let res = f.client().try_initiate_swap_installment(
            &f.token,
            &f.ip_id,
            &f.seller,
            &PRICE,
            &f.seller,
            &2,
        );
        assert_eq!(res, Err(Ok(self_trade_error())));
    }

    #[test]
    fn test_initiate_swap_with_schedule_rejects_self_trade() {
        let f = setup();
        let mut schedule = Vec::new(&f.env);
        schedule.push_back(PaymentSchedule {
            due_timestamp: 1_000,
            amount: PRICE,
        });
        let res = f.client().try_initiate_swap_with_schedule(
            &f.token,
            &f.ip_id,
            &f.seller,
            &schedule,
            &f.seller,
        );
        assert_eq!(res, Err(Ok(self_trade_error())));
    }

    #[test]
    fn test_initiate_escrow_swap_rejects_self_trade() {
        let f = setup();
        let res = f.client().try_initiate_escrow_swap(
            &f.token,
            &f.ip_id,
            &f.seller,
            &PRICE,
            &f.seller,
            &10_000,
        );
        assert_eq!(res, Err(Ok(self_trade_error())));
    }

    #[test]
    fn test_batch_initiate_swap_insured_rejects_self_trade() {
        let f = setup();
        let mut ip_ids = Vec::new(&f.env);
        ip_ids.push_back(f.ip_id);
        let mut prices = Vec::new(&f.env);
        prices.push_back(PRICE);
        let res = f.client().try_batch_initiate_swap_insured(
            &f.token,
            &ip_ids,
            &f.seller,
            &prices,
            &f.seller,
            &1,
            &None::<Address>,
            &false,
        );
        assert_eq!(res, Err(Ok(self_trade_error())));
    }

    #[test]
    fn test_initiate_swap_with_signers_rejects_self_trade() {
        let f = setup();
        let mut signers = Vec::new(&f.env);
        signers.push_back(Address::generate(&f.env));
        let res = f.client().try_initiate_swap_with_signers(
            &f.token,
            &f.ip_id,
            &f.seller,
            &PRICE,
            &f.seller,
            &signers,
        );
        assert_eq!(res, Err(Ok(self_trade_error())));
    }

    #[test]
    fn test_finalize_auction_rejects_seller_as_winning_buyer() {
        let f = setup();
        let auction_id =
            f.client()
                .start_ip_auction(&f.token, &f.ip_id, &f.seller, &PRICE, &3_600);
        // The seller is the only bidder, so finalization would create a self-trade.
        f.client().place_bid(&auction_id, &f.seller, &PRICE);
        f.env
            .ledger()
            .with_mut(|l| l.timestamp += 3_601);

        let res = f.client().try_finalize_auction(&auction_id);
        assert_eq!(res, Err(Ok(self_trade_error())));
    }
}
