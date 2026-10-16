//! Tests for notary key rotation events and history (#1153).

#[cfg(test)]
mod notary_key_rotation_tests {
    use crate::{IpRegistry, IpRegistryClient};
    use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
    use soroban_sdk::{Address, BytesN, Env};

    fn setup() -> (Env, IpRegistryClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, client)
    }

    #[test]
    fn no_history_before_first_key() {
        let (_env, client) = setup();
        assert!(client.get_notary_key_history().is_empty());
        assert!(client.get_notary_key_at(&1000u32).is_none());
    }

    #[test]
    fn key_active_at_ledger_follows_rotations() {
        let (env, client) = setup();
        let key_a = BytesN::from_array(&env, &[0xA1u8; 32]);
        let key_b = BytesN::from_array(&env, &[0xB2u8; 32]);

        env.ledger().set_sequence_number(100);
        client.set_notary_public_key(&key_a);
        env.ledger().set_sequence_number(200);
        client.set_notary_public_key(&key_b);

        assert!(client.get_notary_key_at(&99u32).is_none());
        assert_eq!(client.get_notary_key_at(&100u32), Some(key_a.clone()));
        assert_eq!(client.get_notary_key_at(&199u32), Some(key_a.clone()));
        assert_eq!(client.get_notary_key_at(&200u32), Some(key_b.clone()));
        assert_eq!(client.get_notary_key_at(&5000u32), Some(key_b.clone()));

        let history = client.get_notary_key_history();
        assert_eq!(history.len(), 2);
        assert_eq!(history.get(0).unwrap().effective_ledger, 100);
        assert_eq!(history.get(1).unwrap().public_key, key_b);
    }

    #[test]
    fn same_ledger_rotations_collapse_to_last_key() {
        let (env, client) = setup();
        let key_a = BytesN::from_array(&env, &[0x01u8; 32]);
        let key_b = BytesN::from_array(&env, &[0x02u8; 32]);
        let key_c = BytesN::from_array(&env, &[0x03u8; 32]);

        env.ledger().set_sequence_number(300);
        client.set_notary_public_key(&key_a);
        client.set_notary_public_key(&key_b);
        client.set_notary_public_key(&key_c);

        let history = client.get_notary_key_history();
        assert_eq!(history.len(), 1);
        assert_eq!(client.get_notary_key_at(&300u32), Some(key_c));
    }

    #[test]
    fn re_setting_active_key_is_not_a_rotation() {
        let (env, client) = setup();
        let key_a = BytesN::from_array(&env, &[0x0Au8; 32]);

        env.ledger().set_sequence_number(10);
        client.set_notary_public_key(&key_a);

        env.ledger().set_sequence_number(20);
        client.set_notary_public_key(&key_a);

        assert_eq!(client.get_notary_key_history().len(), 1);
        // `events().all()` reports only the most recent invocation: the no-op re-set emits nothing.
        assert!(env.events().all().events().is_empty());
    }

    #[test]
    fn rotation_emits_event() {
        let (env, client) = setup();
        let key_a = BytesN::from_array(&env, &[0x0Bu8; 32]);
        let key_b = BytesN::from_array(&env, &[0x0Cu8; 32]);

        client.set_notary_public_key(&key_a);
        client.set_notary_public_key(&key_b);
        // The most recent invocation is the rotation, which emits exactly one event.
        assert_eq!(env.events().all().events().len(), 1);
    }
}
