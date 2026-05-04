//! Chaos tests for contract upgrade validation (#376).
//!
//! These tests exercise `check_schema_compatibility` with randomly-mutated
//! schemas to verify that upgrade resilience holds under adversarial inputs:
//! missing functions, scrambled signatures, renumbered errors, dropped storage
//! keys, and arbitrary version combinations.

#[cfg(test)]
mod chaos {
    use soroban_sdk::{Env, String, Vec};

    use crate::upgrade::{
        build_v1_schema, check_schema_compatibility, store_schema, load_schema,
        ContractSchema, ErrorEntry, FunctionEntry,
    };
    use crate::ContractError;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn bump(s: &ContractSchema) -> ContractSchema {
        ContractSchema {
            version: s.version + 1,
            functions: s.functions.clone(),
            errors: s.errors.clone(),
            storage_keys: s.storage_keys.clone(),
        }
    }

    /// Remove the element at `idx` from a `Vec<FunctionEntry>`.
    fn remove_fn_at(env: &Env, v: &Vec<FunctionEntry>, idx: u32) -> Vec<FunctionEntry> {
        let mut out: Vec<FunctionEntry> = Vec::new(env);
        for i in 0..v.len() {
            if i != idx {
                out.push_back(v.get(i).unwrap());
            }
        }
        out
    }

    /// Remove the element at `idx` from a `Vec<ErrorEntry>`.
    fn remove_err_at(env: &Env, v: &Vec<ErrorEntry>, idx: u32) -> Vec<ErrorEntry> {
        let mut out: Vec<ErrorEntry> = Vec::new(env);
        for i in 0..v.len() {
            if i != idx {
                out.push_back(v.get(i).unwrap());
            }
        }
        out
    }

    /// Remove the element at `idx` from a `Vec<String>`.
    fn remove_str_at(env: &Env, v: &Vec<String>, idx: u32) -> Vec<String> {
        let mut out: Vec<String> = Vec::new(env);
        for i in 0..v.len() {
            if i != idx {
                out.push_back(v.get(i).unwrap());
            }
        }
        out
    }

    // ── 1. Upgrade with random WASM (schema-level simulation) ─────────────────

    /// Simulates "random WASM" by supplying a completely empty schema.
    /// Must be rejected on every check.
    #[test]
    fn chaos_empty_schema_rejected() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);

        let empty = ContractSchema {
            version: v1.version + 1,
            functions: Vec::new(&env),
            errors: Vec::new(&env),
            storage_keys: Vec::new(&env),
        };

        assert_eq!(
            check_schema_compatibility(&v1, &empty),
            Err(ContractError::MissingFunc)
        );
    }

    /// Schema with only functions stripped (errors + keys intact) is rejected.
    #[test]
    fn chaos_no_functions_rejected() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let mut bad = bump(&v1);
        bad.functions = Vec::new(&env);

        assert_eq!(
            check_schema_compatibility(&v1, &bad),
            Err(ContractError::MissingFunc)
        );
    }

    /// Schema with only errors stripped is rejected.
    #[test]
    fn chaos_no_errors_rejected() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let mut bad = bump(&v1);
        bad.errors = Vec::new(&env);

        assert_eq!(
            check_schema_compatibility(&v1, &bad),
            Err(ContractError::MissingFunc)
        );
    }

    /// Schema with only storage keys stripped is rejected.
    #[test]
    fn chaos_no_storage_keys_rejected() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let mut bad = bump(&v1);
        bad.storage_keys = Vec::new(&env);

        assert_eq!(
            check_schema_compatibility(&v1, &bad),
            Err(ContractError::MissingFunc)
        );
    }

    // ── 2. State integrity after upgrade ──────────────────────────────────────

    /// After a valid upgrade the stored schema reflects the new version.
    #[test]
    fn chaos_state_integrity_after_valid_upgrade() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        store_schema(&env, &v1);

        let v2 = bump(&v1);
        check_schema_compatibility(&v1, &v2).expect("valid upgrade must pass");
        store_schema(&env, &v2);

        let stored = load_schema(&env).expect("schema must be present");
        assert_eq!(stored.version, v2.version);
        assert_eq!(stored.functions.len(), v2.functions.len());
        assert_eq!(stored.errors.len(), v2.errors.len());
        assert_eq!(stored.storage_keys.len(), v2.storage_keys.len());
    }

    /// A rejected upgrade must NOT mutate the stored schema.
    #[test]
    fn chaos_state_unchanged_after_rejected_upgrade() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        store_schema(&env, &v1);

        // Attempt upgrade with empty schema — must fail.
        let bad = ContractSchema {
            version: v1.version + 1,
            functions: Vec::new(&env),
            errors: Vec::new(&env),
            storage_keys: Vec::new(&env),
        };
        let result = check_schema_compatibility(&v1, &bad);
        assert!(result.is_err());

        // Schema in storage must still be v1.
        let stored = load_schema(&env).expect("schema must still be present");
        assert_eq!(stored.version, v1.version);
    }

    // ── 3. Upgrade resilience — exhaustive single-item removal ────────────────

    /// Removing any single function from the new schema must be rejected.
    #[test]
    fn chaos_every_function_removal_rejected() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let fn_count = v1.functions.len();

        for idx in 0..fn_count {
            let mut candidate = bump(&v1);
            candidate.functions = remove_fn_at(&env, &v1.functions, idx);

            assert_eq!(
                check_schema_compatibility(&v1, &candidate),
                Err(ContractError::MissingFunc),
                "removing function at index {idx} should be rejected"
            );
        }
    }

    /// Removing any single error entry from the new schema must be rejected.
    #[test]
    fn chaos_every_error_removal_rejected() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let err_count = v1.errors.len();

        for idx in 0..err_count {
            let mut candidate = bump(&v1);
            candidate.errors = remove_err_at(&env, &v1.errors, idx);

            assert_eq!(
                check_schema_compatibility(&v1, &candidate),
                Err(ContractError::MissingFunc),
                "removing error at index {idx} should be rejected"
            );
        }
    }

    /// Removing any single storage key from the new schema must be rejected.
    #[test]
    fn chaos_every_storage_key_removal_rejected() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let key_count = v1.storage_keys.len();

        for idx in 0..key_count {
            let mut candidate = bump(&v1);
            candidate.storage_keys = remove_str_at(&env, &v1.storage_keys, idx);

            assert_eq!(
                check_schema_compatibility(&v1, &candidate),
                Err(ContractError::MissingFunc),
                "removing storage key at index {idx} should be rejected"
            );
        }
    }

    // ── 4. Signature scrambling ────────────────────────────────────────────────

    /// Replacing every function's signature with garbage must be rejected.
    #[test]
    fn chaos_all_signatures_scrambled_rejected() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let mut v2 = bump(&v1);

        let mut scrambled: Vec<FunctionEntry> = Vec::new(&env);
        for i in 0..v2.functions.len() {
            let f = v2.functions.get(i).unwrap();
            scrambled.push_back(FunctionEntry {
                name: f.name.clone(),
                signature: String::from_str(&env, "INVALID_SIG"),
            });
        }
        v2.functions = scrambled;

        assert_eq!(
            check_schema_compatibility(&v1, &v2),
            Err(ContractError::UpgradeFunctionSignatureChanged)
        );
    }

    /// Swapping signatures between two functions must be rejected.
    #[test]
    fn chaos_swapped_signatures_rejected() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let mut v2 = bump(&v1);

        // Swap signatures of index 0 and index 1.
        let sig0 = v2.functions.get(0).unwrap().signature.clone();
        let sig1 = v2.functions.get(1).unwrap().signature.clone();

        let mut patched: Vec<FunctionEntry> = Vec::new(&env);
        for i in 0..v2.functions.len() {
            let mut f = v2.functions.get(i).unwrap();
            if i == 0 {
                f.signature = sig1.clone();
            } else if i == 1 {
                f.signature = sig0.clone();
            }
            patched.push_back(f);
        }
        v2.functions = patched;

        assert_eq!(
            check_schema_compatibility(&v1, &v2),
            Err(ContractError::UpgradeFunctionSignatureChanged)
        );
    }

    // ── 5. Error code renumbering ──────────────────────────────────────────────

    /// Incrementing every error code by 1 must be rejected.
    #[test]
    fn chaos_all_error_codes_shifted_rejected() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let mut v2 = bump(&v1);

        let mut shifted: Vec<ErrorEntry> = Vec::new(&env);
        for i in 0..v2.errors.len() {
            let e = v2.errors.get(i).unwrap();
            shifted.push_back(ErrorEntry {
                name: e.name.clone(),
                code: e.code + 100,
            });
        }
        v2.errors = shifted;

        assert_eq!(
            check_schema_compatibility(&v1, &v2),
            Err(ContractError::FuncChanged)
        );
    }

    // ── 6. Version boundary chaos ──────────────────────────────────────────────

    /// Version u32::MAX in current — any new schema overflows or stays ≤ MAX,
    /// so it must be rejected.
    #[test]
    fn chaos_version_overflow_rejected() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);

        let maxed = ContractSchema {
            version: u32::MAX,
            functions: v1.functions.clone(),
            errors: v1.errors.clone(),
            storage_keys: v1.storage_keys.clone(),
        };

        // Any candidate with version ≤ u32::MAX must fail the version gate.
        let candidate = ContractSchema {
            version: u32::MAX,
            ..maxed.clone()
        };
        assert_eq!(
            check_schema_compatibility(&maxed, &candidate),
            Err(ContractError::SchemaNotGreater)
        );
    }

    /// Version 0 in new schema (below any valid current) must be rejected.
    #[test]
    fn chaos_zero_version_rejected() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);

        let zero = ContractSchema {
            version: 0,
            functions: v1.functions.clone(),
            errors: v1.errors.clone(),
            storage_keys: v1.storage_keys.clone(),
        };

        assert_eq!(
            check_schema_compatibility(&v1, &zero),
            Err(ContractError::SchemaNotGreater)
        );
    }

    // ── 7. Multi-step upgrade chain integrity ─────────────────────────────────

    /// Three sequential valid upgrades must all pass and leave the correct
    /// version in storage.
    #[test]
    fn chaos_multi_step_upgrade_chain() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        store_schema(&env, &v1);

        for expected_version in 2u32..=4 {
            let current = load_schema(&env).unwrap();
            let next = bump(&current);
            assert_eq!(check_schema_compatibility(&current, &next), Ok(()));
            store_schema(&env, &next);
            assert_eq!(load_schema(&env).unwrap().version, expected_version);
        }
    }

    /// An upgrade that skips a version (v1 → v3) must still pass — only
    /// monotonic increase is required, not consecutive.
    #[test]
    fn chaos_version_skip_allowed() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);

        let v3 = ContractSchema {
            version: v1.version + 10,
            functions: v1.functions.clone(),
            errors: v1.errors.clone(),
            storage_keys: v1.storage_keys.clone(),
        };

        assert_eq!(check_schema_compatibility(&v1, &v3), Ok(()));
    }
}
