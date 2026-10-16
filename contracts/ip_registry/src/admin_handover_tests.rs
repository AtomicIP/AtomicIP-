//! Tests for the two-step admin handover (#1154).

#[cfg(test)]
mod admin_handover_tests {
    use crate::{IpRegistry, IpRegistryClient};
    use soroban_sdk::testutils::{Address as _, Events as _, MockAuth, MockAuthInvoke};
    use soroban_sdk::{Address, Env, IntoVal, Val, Vec};

    fn setup() -> (Env, IpRegistryClient<'static>, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, client, admin, contract_id)
    }

    /// Authorize exactly one address for exactly one call. Anything else
    /// that calls `require_auth` will fail.
    fn only_authorize(
        env: &Env,
        contract_id: &Address,
        signer: &Address,
        fn_name: &'static str,
        args: Vec<Val>,
    ) {
        env.mock_auths(&[MockAuth {
            address: signer,
            invoke: &MockAuthInvoke {
                contract: contract_id,
                fn_name,
                args,
                sub_invokes: &[],
            },
        }]);
    }

    #[test]
    fn propose_does_not_move_admin_rights() {
        let (env, client, admin, contract_id) = setup();
        let nominee = Address::generate(&env);
        client.propose_admin(&nominee);

        assert_eq!(client.get_pending_admin(), Some(nominee.clone()));
        // The current admin still controls admin-gated functions.
        only_authorize(
            &env,
            &contract_id,
            &admin,
            "set_upgrade_timelock",
            soroban_sdk::vec![&env, 5u32.into_val(&env)],
        );
        client.set_upgrade_timelock(&5u32);
        assert_eq!(client.get_upgrade_timelock(), 5);
    }

    #[test]
    fn accept_requires_nominee_auth() {
        let (env, client, _admin, _contract_id) = setup();
        let nominee = Address::generate(&env);
        client.propose_admin(&nominee);

        // Only the admin is authorized; accepting must still fail.
        env.mock_auths(&[]);
        assert!(client.try_accept_admin().is_err());
        assert_eq!(client.get_pending_admin(), Some(nominee.clone()));
    }

    #[test]
    fn accept_moves_admin_rights_to_nominee() {
        let (env, client, admin, contract_id) = setup();
        let nominee = Address::generate(&env);
        client.propose_admin(&nominee);

        only_authorize(
            &env,
            &contract_id,
            &nominee,
            "accept_admin",
            soroban_sdk::vec![&env],
        );
        client.accept_admin();
        assert!(client.get_pending_admin().is_none());

        // The nominee now controls admin functions...
        only_authorize(
            &env,
            &contract_id,
            &nominee,
            "set_upgrade_timelock",
            soroban_sdk::vec![&env, 10u32.into_val(&env)],
        );
        client.set_upgrade_timelock(&10u32);
        assert_eq!(client.get_upgrade_timelock(), 10);

        // ...and the old admin no longer does.
        only_authorize(
            &env,
            &contract_id,
            &admin,
            "set_upgrade_timelock",
            soroban_sdk::vec![&env, 20u32.into_val(&env)],
        );
        assert!(client.try_set_upgrade_timelock(&20u32).is_err());
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #65)")]
    fn accept_without_nomination_rejected() {
        let (_env, client, _admin, _contract_id) = setup();
        client.accept_admin();
    }

    #[test]
    fn cancel_withdraws_nomination_and_keeps_admin() {
        let (env, client, admin, contract_id) = setup();
        let nominee = Address::generate(&env);
        client.propose_admin(&nominee);
        client.cancel_admin_proposal();

        assert!(client.get_pending_admin().is_none());

        // The withdrawn nominee can no longer accept.
        only_authorize(
            &env,
            &contract_id,
            &nominee,
            "accept_admin",
            soroban_sdk::vec![&env],
        );
        assert!(client.try_accept_admin().is_err());

        // The original admin keeps control.
        only_authorize(
            &env,
            &contract_id,
            &admin,
            "set_upgrade_timelock",
            soroban_sdk::vec![&env, 5u32.into_val(&env)],
        );
        client.set_upgrade_timelock(&5u32);
        assert_eq!(client.get_upgrade_timelock(), 5);
    }

    #[test]
    fn cancel_without_nomination_is_noop() {
        let (_env, client, _admin, _contract_id) = setup();
        client.cancel_admin_proposal();
        assert!(client.get_pending_admin().is_none());
    }

    #[test]
    fn wrong_address_nomination_is_replaced_and_cannot_accept() {
        // A typo'd nominee must not lock the admin out: the admin re-nominates
        // the correct address, and the typo'd address can no longer accept.
        let (env, client, _admin, contract_id) = setup();
        let typo = Address::generate(&env);
        let correct = Address::generate(&env);

        client.propose_admin(&typo);
        client.propose_admin(&correct);
        assert_eq!(client.get_pending_admin(), Some(correct.clone()));

        only_authorize(
            &env,
            &contract_id,
            &typo,
            "accept_admin",
            soroban_sdk::vec![&env],
        );
        assert!(client.try_accept_admin().is_err());

        only_authorize(
            &env,
            &contract_id,
            &correct,
            "accept_admin",
            soroban_sdk::vec![&env],
        );
        client.accept_admin();
        assert!(client.get_pending_admin().is_none());
    }

    #[test]
    fn propose_emits_event() {
        let (env, client, _admin, _contract_id) = setup();
        client.propose_admin(&Address::generate(&env));
        assert_eq!(env.events().all().events().len(), 1);
    }

    #[test]
    fn non_admin_cannot_propose() {
        let (env, client, _admin, contract_id) = setup();
        let attacker = Address::generate(&env);

        only_authorize(
            &env,
            &contract_id,
            &attacker,
            "propose_admin",
            soroban_sdk::vec![&env, attacker.clone().into_val(&env)],
        );
        assert!(client.try_propose_admin(&attacker).is_err());
        assert!(client.get_pending_admin().is_none());
    }
}
