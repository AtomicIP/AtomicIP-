//! Tests for the upgrade timelock (#1152).
//!
//! The successful `upgrade` path replaces the installed WASM, which the unit
//! test host cannot do (see `upgrade_tests.rs`). These tests cover the
//! proposal, cancellation, configuration, and gating logic that runs before
//! the WASM is replaced.

#[cfg(test)]
mod upgrade_timelock_tests {
    use crate::{IpRegistry, IpRegistryClient};
    use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
    use soroban_sdk::{Address, BytesN, Env};

    fn setup() -> (Env, IpRegistryClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        env.ledger().set_sequence_number(100);
        (env, client, admin)
    }

    #[test]
    fn timelock_defaults_to_disabled() {
        let (_env, client, _admin) = setup();
        assert_eq!(client.get_upgrade_timelock(), 0);
        assert!(client.get_pending_upgrade().is_none());
    }

    #[test]
    fn set_upgrade_timelock_persists() {
        let (_env, client, _admin) = setup();
        client.set_upgrade_timelock(&50u32);
        assert_eq!(client.get_upgrade_timelock(), 50);
    }

    #[test]
    fn propose_upgrade_sets_executable_ledger_after_delay() {
        let (env, client, _admin) = setup();
        client.set_upgrade_timelock(&50u32);

        let hash = BytesN::from_array(&env, &[0x11u8; 32]);
        let executable = client.propose_upgrade(&hash);

        // proposed at ledger 100 + 50-ledger timelock
        assert_eq!(executable, 150);
        let pending = client.get_pending_upgrade().unwrap();
        assert_eq!(pending.wasm_hash, hash);
        assert_eq!(pending.proposed_ledger, 100);
        assert_eq!(pending.executable_ledger, 150);
    }

    #[test]
    fn propose_upgrade_with_timelock_disabled_is_immediately_executable() {
        let (env, client, _admin) = setup();
        let hash = BytesN::from_array(&env, &[0x12u8; 32]);
        assert_eq!(client.propose_upgrade(&hash), 100);
    }

    #[test]
    fn propose_upgrade_replaces_previous_proposal() {
        let (env, client, _admin) = setup();
        client.set_upgrade_timelock(&10u32);
        client.propose_upgrade(&BytesN::from_array(&env, &[0x21u8; 32]));
        let second = BytesN::from_array(&env, &[0x22u8; 32]);
        client.propose_upgrade(&second);
        assert_eq!(client.get_pending_upgrade().unwrap().wasm_hash, second);
    }

    #[test]
    fn cancel_upgrade_clears_pending_proposal() {
        let (env, client, _admin) = setup();
        client.set_upgrade_timelock(&10u32);
        client.propose_upgrade(&BytesN::from_array(&env, &[0x31u8; 32]));
        client.cancel_upgrade();
        assert!(client.get_pending_upgrade().is_none());
    }

    #[test]
    fn cancel_upgrade_without_proposal_is_noop() {
        let (_env, client, _admin) = setup();
        client.cancel_upgrade();
        assert!(client.get_pending_upgrade().is_none());
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #64)")]
    fn upgrade_without_proposal_rejected_when_timelock_enabled() {
        let (env, client, _admin) = setup();
        client.set_upgrade_timelock(&10u32);
        client.upgrade(&BytesN::from_array(&env, &[0x41u8; 32]));
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #64)")]
    fn upgrade_with_different_hash_than_proposed_rejected() {
        let (env, client, _admin) = setup();
        client.set_upgrade_timelock(&10u32);
        client.propose_upgrade(&BytesN::from_array(&env, &[0x51u8; 32]));
        client.upgrade(&BytesN::from_array(&env, &[0x52u8; 32]));
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #39)")]
    fn upgrade_before_timelock_expires_rejected() {
        let (env, client, _admin) = setup();
        client.set_upgrade_timelock(&50u32);
        let hash = BytesN::from_array(&env, &[0x61u8; 32]);
        client.propose_upgrade(&hash);
        env.ledger().set_sequence_number(149); // one ledger short of 150
        client.upgrade(&hash);
    }

    #[test]
    fn propose_upgrade_emits_event() {
        let (env, client, _admin) = setup();
        client.set_upgrade_timelock(&5u32);
        let before = env.events().all().events().len();
        client.propose_upgrade(&BytesN::from_array(&env, &[0x71u8; 32]));
        assert!(env.events().all().events().len() > before);
    }
}
