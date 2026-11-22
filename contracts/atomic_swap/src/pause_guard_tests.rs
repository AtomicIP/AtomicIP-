//! #1349: Fund-moving entry points must reject calls while the contract is
//! paused (`ContractPaused` = 21). Each test pauses the contract and asserts
//! the call fails with that error. Ids are unknown on purpose: the pause check
//! must fire before any state lookup.

#[cfg(test)]
mod pause_guard_tests {
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

    use crate::{AtomicSwap, AtomicSwapClient};

    const CONTRACT_PAUSED: u32 = 21;

    fn paused_error() -> soroban_sdk::Error {
        soroban_sdk::Error::from_contract_error(CONTRACT_PAUSED)
    }

    struct Fixture {
        env: Env,
        contract_id: Address,
        admin: Address,
    }

    impl Fixture {
        fn client(&self) -> AtomicSwapClient<'_> {
            AtomicSwapClient::new(&self.env, &self.contract_id)
        }
    }

    /// Initializes the contract with an admin and pauses it.
    fn paused_contract() -> Fixture {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let registry_id = Address::generate(&env);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&admin, &registry_id);
        client.set_admin(&admin);
        client.pause(&admin);

        Fixture {
            env,
            contract_id,
            admin,
        }
    }

    fn bytes32(env: &Env, seed: u8) -> BytesN<32> {
        BytesN::from_array(env, &[seed; 32])
    }

    // ── Deposits and payments ─────────────────────────────────────────────────

    #[test]
    fn test_escrow_deposit_rejected_when_paused() {
        let f = paused_contract();
        let res = f.client().try_escrow_deposit(&1);
        assert_eq!(res, Err(Ok(paused_error())));
    }

    #[test]
    fn test_batch_escrow_deposit_rejected_when_paused() {
        let f = paused_contract();
        let mut swap_ids = Vec::new(&f.env);
        swap_ids.push_back(1u64);
        let buyer = Address::generate(&f.env);
        let res = f.client().try_batch_escrow_deposit(&swap_ids, &buyer);
        assert_eq!(res, Err(Ok(paused_error())));
    }

    #[test]
    fn test_submit_installment_payment_rejected_when_paused() {
        let f = paused_contract();
        let res = f.client().try_submit_installment_payment(&1, &100);
        assert_eq!(res, Err(Ok(paused_error())));
    }

    #[test]
    fn test_make_scheduled_payment_rejected_when_paused() {
        let f = paused_contract();
        let res = f.client().try_make_scheduled_payment(&1, &0);
        assert_eq!(res, Err(Ok(paused_error())));
    }

    #[test]
    fn test_fund_insurance_pool_rejected_when_paused() {
        let f = paused_contract();
        let funder = Address::generate(&f.env);
        let token = Address::generate(&f.env);
        let res = f.client().try_fund_insurance_pool(&funder, &token, &100);
        assert_eq!(res, Err(Ok(paused_error())));
    }

    // ── Auctions ──────────────────────────────────────────────────────────────

    #[test]
    fn test_place_bid_rejected_when_paused() {
        let f = paused_contract();
        let bidder = Address::generate(&f.env);
        let res = f.client().try_place_bid(&1, &bidder, &100);
        assert_eq!(res, Err(Ok(paused_error())));
    }

    // ── Key reveal and completion ─────────────────────────────────────────────

    #[test]
    fn test_reveal_key_rejected_when_paused() {
        let f = paused_contract();
        let caller = Address::generate(&f.env);
        let res = f.client().try_reveal_key(
            &1,
            &caller,
            &bytes32(&f.env, 1),
            &bytes32(&f.env, 2),
        );
        assert_eq!(res, Err(Ok(paused_error())));
    }

    #[test]
    fn test_batch_reveal_keys_rejected_when_paused() {
        let f = paused_contract();
        let seller = Address::generate(&f.env);
        let mut swap_ids = Vec::new(&f.env);
        swap_ids.push_back(1u64);
        let mut secrets = Vec::new(&f.env);
        secrets.push_back(bytes32(&f.env, 1));
        let mut blinding = Vec::new(&f.env);
        blinding.push_back(bytes32(&f.env, 2));
        let res = f
            .client()
            .try_batch_reveal_keys(&swap_ids, &secrets, &blinding, &seller);
        assert_eq!(res, Err(Ok(paused_error())));
    }

    #[test]
    fn test_verify_and_complete_swap_rejected_when_paused() {
        let f = paused_contract();
        let caller = Address::generate(&f.env);
        let res = f.client().try_verify_and_complete_swap(
            &1,
            &caller,
            &bytes32(&f.env, 1),
            &bytes32(&f.env, 2),
        );
        assert_eq!(res, Err(Ok(paused_error())));
    }

    // ── Disputes ──────────────────────────────────────────────────────────────

    #[test]
    fn test_execute_ruling_rejected_when_paused() {
        let f = paused_contract();
        let res = f.client().try_execute_ruling(&1);
        assert_eq!(res, Err(Ok(paused_error())));
    }

    // ── Control: the same call is not blocked by the pause once unpaused ──────

    #[test]
    fn test_escrow_deposit_not_blocked_by_pause_after_unpause() {
        let f = paused_contract();
        f.client().unpause(&f.admin);
        let res = f.client().try_escrow_deposit(&1);
        assert_ne!(res, Err(Ok(paused_error())));
    }
}
