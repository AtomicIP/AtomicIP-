//! #1348: `num_installments` must be enforced by `submit_installment_payment`,
//! and zero-amount / overpayments must not be reported as `PriceTooSmall` (3).

#[cfg(test)]
mod installment_limit_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _, token::StellarAssetClient, Address, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient};

    const PRICE: i128 = 1_000;
    const PRICE_TOO_SMALL: u32 = 3;

    struct Fixture {
        env: Env,
        contract_id: Address,
        buyer: Address,
        token: Address,
        swap_id: u64,
    }

    impl Fixture {
        fn client(&self) -> AtomicSwapClient<'_> {
            AtomicSwapClient::new(&self.env, &self.contract_id)
        }
    }

    /// Creates an installment swap at `PRICE` split into `num_installments` payments.
    fn setup(num_installments: u32) -> Fixture {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let hash = BytesN::from_array(&env, &[0x48; 32]);
        let ip_id = IpRegistryClient::new(&env, &registry_id).commit_ip(&seller, &hash, &0u32);

        let token = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(&env, &token).mint(&buyer, &1_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&admin, &registry_id);

        let swap_id = client.initiate_swap_installment(
            &token,
            &ip_id,
            &seller,
            &PRICE,
            &buyer,
            &num_installments,
        );

        Fixture {
            env,
            contract_id,
            buyer,
            token,
            swap_id,
        }
    }

    // ── Control: payments within the agreed count are accepted ────────────────

    #[test]
    fn test_payments_within_installment_count_are_accepted() {
        let f = setup(2);
        f.client().submit_installment_payment(&f.swap_id, &100);
        f.client().submit_installment_payment(&f.swap_id, &100);

        let (paid, price, remaining) = f.client().get_installment_status(&f.swap_id);
        assert_eq!(paid, 200);
        assert_eq!(price, PRICE);
        assert_eq!(remaining, PRICE - 200);
    }

    // ── num_installments is enforced ───────────────────────────────────────────

    #[test]
    fn test_payment_beyond_num_installments_is_rejected() {
        let f = setup(2);
        f.client().submit_installment_payment(&f.swap_id, &100);
        f.client().submit_installment_payment(&f.swap_id, &100);

        // The seller agreed to two installments; a third must fail even though
        // the swap still has balance remaining.
        let res = f
            .client()
            .try_submit_installment_payment(&f.swap_id, &100);
        assert!(res.is_err());

        let (paid, _, _) = f.client().get_installment_status(&f.swap_id);
        assert_eq!(paid, 200, "rejected payment must not change paid_amount");
    }

    #[test]
    fn test_single_installment_swap_rejects_second_partial_payment() {
        let f = setup(1);
        f.client().submit_installment_payment(&f.swap_id, &100);

        let res = f
            .client()
            .try_submit_installment_payment(&f.swap_id, &100);
        assert!(res.is_err());
    }

    // ── Dedicated error codes for zero and overpayment ────────────────────────

    #[test]
    fn test_zero_payment_does_not_report_price_too_small() {
        let f = setup(2);
        let res = f.client().try_submit_installment_payment(&f.swap_id, &0);
        assert!(res.is_err());
        assert_ne!(
            res,
            Err(Ok(soroban_sdk::Error::from_contract_error(PRICE_TOO_SMALL)))
        );
    }

    #[test]
    fn test_overpayment_does_not_report_price_too_small() {
        let f = setup(2);
        let res = f
            .client()
            .try_submit_installment_payment(&f.swap_id, &(PRICE + 1));
        assert!(res.is_err());
        assert_ne!(
            res,
            Err(Ok(soroban_sdk::Error::from_contract_error(PRICE_TOO_SMALL)))
        );
    }

    #[test]
    fn test_rejected_overpayment_transfers_no_tokens() {
        let f = setup(2);
        let token = soroban_sdk::token::Client::new(&f.env, &f.token);
        let before = token.balance(&f.buyer);

        let _ = f
            .client()
            .try_submit_installment_payment(&f.swap_id, &(PRICE + 1));

        assert_eq!(token.balance(&f.buyer), before);
        assert_eq!(token.balance(&f.contract_id), 0);
    }
}
