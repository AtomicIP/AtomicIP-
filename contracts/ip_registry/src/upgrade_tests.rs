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
    use crate::{ErrorCodeEntry, FunctionEntry, IpRecord, IpRegistry, UpgradeManifest};
    use soroban_sdk::contractclient;
    use soroban_sdk::testutils::Address as TestAddress;
    use soroban_sdk::{Address, BytesN, Env, String, Symbol};

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
        fn validate_upgrade(env: Env, new_wasm_hash: BytesN<32>, manifest: UpgradeManifest);
        fn upgrade(env: Env, new_wasm_hash: BytesN<32>);
    }

    fn setup() -> (Env, UpgradeTestClient<'static>) {
        let env = Env::default();
        let contract_id = env.register(crate::IpRegistry, ());
        let client = UpgradeTestClient::new(&env, &contract_id);
        (env, client)
    }

    /// A manifest that faithfully describes the contract's own current
    /// interface — must always pass `validate_upgrade` (given a non-zero
    /// hash), since it changes nothing.
    fn full_manifest(env: &Env) -> UpgradeManifest {
        UpgradeManifest {
            exported_functions: IpRegistry::required_exported_functions(env),
            error_codes: IpRegistry::current_error_codes(env),
            storage_keys: IpRegistry::current_storage_keys(env),
        }
    }

    // ── validate_upgrade: acceptance ──────────────────────────────────────────

    #[test]
    fn validate_upgrade_accepts_typical_hash() {
        let (env, client) = setup();
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        // Must not panic.
        client.validate_upgrade(&hash, &full_manifest(&env));
    }

    #[test]
    fn validate_upgrade_accepts_all_ones_hash() {
        let (env, client) = setup();
        let hash = BytesN::from_array(&env, &[0xffu8; 32]);
        client.validate_upgrade(&hash, &full_manifest(&env));
    }

    #[test]
    fn validate_upgrade_accepts_single_nonzero_byte() {
        let (env, client) = setup();
        let mut bytes = [0u8; 32];
        bytes[31] = 1; // smallest non-zero hash
        let hash = BytesN::from_array(&env, &bytes);
        client.validate_upgrade(&hash, &full_manifest(&env));
    }

    #[test]
    fn validate_upgrade_accepts_additive_changes() {
        // Adding a brand-new function, error code, and storage key is not
        // a breaking change — everything the current interface requires is
        // still present.
        let (env, client) = setup();
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let mut manifest = full_manifest(&env);
        manifest.exported_functions.push_back(FunctionEntry {
            name: Symbol::new(&env, "new_query"),
            signature: String::from_str(&env, "new_query(id:u64)->bool"),
        });
        manifest.error_codes.push_back(ErrorCodeEntry {
            name: Symbol::new(&env, "NewError"),
            code: 999,
        });
        manifest.storage_keys.push_back(Symbol::new(&env, "NewIndex"));
        client.validate_upgrade(&hash, &manifest);
    }

    // ── validate_upgrade: rejection ───────────────────────────────────────────

    #[test]
    #[should_panic(expected = "Error(Contract, #5)")]
    fn validate_upgrade_rejects_zero_hash() {
        let (env, client) = setup();
        let zero = BytesN::from_array(&env, &[0u8; 32]);
        client.validate_upgrade(&zero, &full_manifest(&env));
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #5)")]
    fn validate_upgrade_rejects_missing_required_function() {
        // A deliberately incompatible candidate: `commit_ip` dropped from
        // the manifest, standing in for a candidate WASM that no longer
        // exports it.
        let (env, client) = setup();
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let mut manifest = full_manifest(&env);
        let commit_ip = Symbol::new(&env, "commit_ip");
        let mut trimmed: soroban_sdk::Vec<FunctionEntry> = soroban_sdk::Vec::new(&env);
        for f in manifest.exported_functions.iter() {
            if f.name != commit_ip {
                trimmed.push_back(f);
            }
        }
        manifest.exported_functions = trimmed;
        client.validate_upgrade(&hash, &manifest);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #5)")]
    fn validate_upgrade_rejects_changed_function_signature() {
        // A deliberately incompatible candidate: `get_ip`'s signature
        // changed, standing in for a candidate WASM with a breaking
        // parameter/return-type change on a required function.
        let (env, client) = setup();
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let mut manifest = full_manifest(&env);
        let get_ip = Symbol::new(&env, "get_ip");
        let mut patched: soroban_sdk::Vec<FunctionEntry> = soroban_sdk::Vec::new(&env);
        for mut f in manifest.exported_functions.iter() {
            if f.name == get_ip {
                f.signature = String::from_str(&env, "get_ip(ip_id:u64)->Option<IpRecord>");
            }
            patched.push_back(f);
        }
        manifest.exported_functions = patched;
        client.validate_upgrade(&hash, &manifest);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #5)")]
    fn validate_upgrade_rejects_missing_error_code() {
        // A deliberately incompatible candidate: `IpNotFound` dropped from
        // the manifest, standing in for a candidate WASM that removed an
        // error variant clients pattern-match on.
        let (env, client) = setup();
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let mut manifest = full_manifest(&env);
        let ip_not_found = Symbol::new(&env, "IpNotFound");
        let mut trimmed: soroban_sdk::Vec<ErrorCodeEntry> = soroban_sdk::Vec::new(&env);
        for e in manifest.error_codes.iter() {
            if e.name != ip_not_found {
                trimmed.push_back(e);
            }
        }
        manifest.error_codes = trimmed;
        client.validate_upgrade(&hash, &manifest);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #5)")]
    fn validate_upgrade_rejects_renumbered_error_code() {
        // A deliberately incompatible candidate: `IpNotFound` renumbered
        // from 1 to 99, standing in for a candidate WASM whose error codes
        // shifted — silently breaking any client matching on the old value.
        let (env, client) = setup();
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let mut manifest = full_manifest(&env);
        let ip_not_found = Symbol::new(&env, "IpNotFound");
        let mut patched: soroban_sdk::Vec<ErrorCodeEntry> = soroban_sdk::Vec::new(&env);
        for mut e in manifest.error_codes.iter() {
            if e.name == ip_not_found {
                e.code = 99;
            }
            patched.push_back(e);
        }
        manifest.error_codes = patched;
        client.validate_upgrade(&hash, &manifest);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #5)")]
    fn validate_upgrade_rejects_missing_storage_key() {
        // A deliberately incompatible candidate: the `Admin` storage key
        // dropped from the manifest, standing in for a candidate WASM that
        // stopped declaring a storage key existing records rely on.
        let (env, client) = setup();
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let mut manifest = full_manifest(&env);
        let admin = Symbol::new(&env, "Admin");
        let mut trimmed: soroban_sdk::Vec<Symbol> = soroban_sdk::Vec::new(&env);
        for k in manifest.storage_keys.iter() {
            if k != admin {
                trimmed.push_back(k);
            }
        }
        manifest.storage_keys = trimmed;
        client.validate_upgrade(&hash, &manifest);
    }

    // ── validate_upgrade is repeatable / side-effect free ─────────────────────

    #[test]
    fn validate_upgrade_is_idempotent() {
        let (env, client) = setup();
        let hash = BytesN::from_array(&env, &[7u8; 32]);
        // Calling the compatibility check repeatedly is always safe.
        for _ in 0..5 {
            client.validate_upgrade(&hash, &full_manifest(&env));
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
        client.validate_upgrade(&candidate, &full_manifest(&env));

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
