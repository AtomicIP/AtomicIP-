//! Contract upgrade compatibility tests (#557).
//!
//! These tests cover the upgrade-safety surface of the IP Registry contract:
//!
//! * `validate_upgrade` — the compatibility gate that must accept a well-formed
//!   candidate WASM hash and reject an obviously invalid (zero) one. A zero hash
//!   stands in for "no/garbage WASM" and must never be accepted.
//! * State preservation — running the compatibility check must be a pure,
//!   read-only operation: committed IP records and ID allocation are unchanged
//!   by it. This is the property an operator relies on when validating a
//!   candidate upgrade against live state.
//! * Authorization — `upgrade` must refuse to run when no admin has been
//!   established, so an un-initialized contract can never be upgraded by an
//!   unauthorized caller.
//!
//! The successful `upgrade` path (`update_current_contract_wasm`) is exercised
//! on-chain rather than here: it requires a genuinely installed WASM hash, which
//! the unit-test host cannot provide. The compatibility and authorization logic
//! that guards it is what these tests pin down.

#[cfg(test)]
mod upgrade_tests {
    use crate::IpRecord;
    use soroban_sdk::contractclient;
    use soroban_sdk::testutils::Address as TestAddress;
    use soroban_sdk::{Address, BytesN, Env};

    #[contractclient(name = "UpgradeTestClient")]
    #[allow(dead_code)]
    pub trait UpgradeIface {
        fn commit_ip(
            env: Env,
            owner: Address,
            commitment_hash: BytesN<32>,
            pow_difficulty: u32,
        ) -> u64;
        fn get_ip(env: Env, ip_id: u64) -> IpRecord;
        fn validate_upgrade(env: Env, new_wasm_hash: BytesN<32>);
        fn upgrade(env: Env, new_wasm_hash: BytesN<32>);
    }

    fn setup() -> (Env, UpgradeTestClient<'static>) {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = UpgradeTestClient::new(&env, &contract_id);
        (env, client)
    }

    // ── validate_upgrade: acceptance ──────────────────────────────────────────

    #[test]
    fn validate_upgrade_accepts_typical_hash() {
        let (env, client) = setup();
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        // Must not panic.
        client.validate_upgrade(&hash);
    }

    #[test]
    fn validate_upgrade_accepts_all_ones_hash() {
        let (env, client) = setup();
        let hash = BytesN::from_array(&env, &[0xffu8; 32]);
        client.validate_upgrade(&hash);
    }

    #[test]
    fn validate_upgrade_accepts_single_nonzero_byte() {
        let (env, client) = setup();
        let mut bytes = [0u8; 32];
        bytes[31] = 1; // smallest non-zero hash
        let hash = BytesN::from_array(&env, &bytes);
        client.validate_upgrade(&hash);
    }

    // ── validate_upgrade: rejection ───────────────────────────────────────────

    #[test]
    #[should_panic(expected = "Error(Contract, #5)")]
    fn validate_upgrade_rejects_zero_hash() {
        let (env, client) = setup();
        let zero = BytesN::from_array(&env, &[0u8; 32]);
        client.validate_upgrade(&zero);
    }

    // ── validate_upgrade is repeatable / side-effect free ─────────────────────

    #[test]
    fn validate_upgrade_is_idempotent() {
        let (env, client) = setup();
        let hash = BytesN::from_array(&env, &[7u8; 32]);
        // Calling the compatibility check repeatedly is always safe.
        for _ in 0..5 {
            client.validate_upgrade(&hash);
        }
    }

    // ── State preservation across the compatibility check ─────────────────────

    #[test]
    fn validate_upgrade_preserves_committed_state() {
        let (env, client) = setup();
        env.mock_all_auths();

        let owner = <Address as TestAddress>::generate(&env);
        let h1 = BytesN::from_array(&env, &[11u8; 32]);
        let h2 = BytesN::from_array(&env, &[22u8; 32]);

        let id1 = client.commit_ip(&owner, &h1, &0u32);
        let id2 = client.commit_ip(&owner, &h2, &0u32);

        // Run the upgrade compatibility gate against live state.
        let candidate = BytesN::from_array(&env, &[9u8; 32]);
        client.validate_upgrade(&candidate);

        // Records and ID allocation must be untouched by the validation.
        let r1 = client.get_ip(&id1);
        let r2 = client.get_ip(&id2);
        assert_eq!(r1.commitment_hash, h1);
        assert_eq!(r2.commitment_hash, h2);
        assert_eq!(r1.owner, owner);
        assert_eq!(r2.owner, owner);

        // The next allocated ID continues the sequence — no IDs were consumed.
        let id3 = client.commit_ip(&owner, &BytesN::from_array(&env, &[33u8; 32]), &0u32);
        assert_eq!(id3, id2 + 1);
    }

    // ── Authorization guard on upgrade ────────────────────────────────────────

    #[test]
    #[should_panic(expected = "Error(Contract, #5)")]
    fn upgrade_rejected_when_no_admin_initialized() {
        // A fresh contract has never had `commit_ip` called, so no admin exists.
        // `upgrade` must refuse rather than allow an unauthorized upgrade.
        let (env, client) = setup();
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        client.upgrade(&hash);
    }
}
