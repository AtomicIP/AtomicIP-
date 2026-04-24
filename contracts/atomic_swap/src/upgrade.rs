//! Upgrade validation for the AtomicSwap contract.
//!
//! # Safety Model
//!
//! Soroban does not expose runtime WASM introspection — you cannot enumerate
//! exported symbols from a hash at execution time.  The approach taken here is
//! **schema-manifest comparison**:
//!
//! 1. The *current* contract embeds its interface manifest as a [`ContractSchema`]
//!    stored in instance storage under `DataKey::ContractSchema`.
//! 2. Before upgrading, the admin calls
//!    `AtomicSwap::validate_upgrade(env, new_wasm_hash, new_schema)` with the
//!    manifest of the *candidate* WASM.
//! 3. [`check_schema_compatibility`] verifies the candidate manifest is a strict
//!    superset of the current one — no function removed, no error code changed,
//!    no storage key removed.
//! 4. Only after all checks pass does the caller invoke
//!    `env.deployer().update_current_contract_wasm(new_wasm_hash)`.
//!
//! # What Must Not Change Across Upgrades
//!
//! | Category | Rule |
//! |---|---|
//! | Function names | Every name in the current schema must appear in the new schema |
//! | Function signatures | Signatures must be byte-identical (additions allowed) |
//! | Error codes | Every `(name, discriminant)` pair must be preserved |
//! | Storage keys | Every key variant name must remain present |
//!
//! # Assumptions / Limitations
//!
//! The manifest is supplied by the upgrader, not extracted from the WASM binary,
//! because Soroban provides no on-chain WASM disassembly API.  The admin is
//! trusted to supply an accurate manifest; the on-chain check prevents
//! *accidental* breaking changes.

use soroban_sdk::{contracttype, BytesN, Env, String, Vec};

use crate::{ContractError, DataKey};

// ── Schema types ──────────────────────────────────────────────────────────────

/// A single exported function entry in the interface manifest.
///
/// `signature` encodes the full function signature as a deterministic string,
/// e.g. `"initiate_swap(token:Address,ip_id:u64,...)->u64"`.
/// Any change — including argument reordering — is treated as a breaking change.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionEntry {
    /// Exported function name.
    pub name: String,
    /// Full signature string (deterministic encoding).
    pub signature: String,
}

/// A single error-code entry in the interface manifest.
///
/// Both the symbolic name *and* the numeric discriminant must be preserved
/// across upgrades so that off-chain clients that pattern-match on error codes
/// continue to work correctly.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ErrorEntry {
    /// Symbolic error name, e.g. `"SwapNotFound"`.
    pub name: String,
    /// Numeric discriminant, e.g. `1`.
    pub code: u32,
}

/// The complete on-chain interface manifest for one version of the contract.
///
/// Stored under `DataKey::ContractSchema` in instance storage.  Updated after
/// every successful upgrade so the next upgrade can validate against it.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ContractSchema {
    /// Monotonically increasing schema version.  The new schema's version must
    /// be strictly greater than the current one.
    pub version: u32,
    /// All exported public functions.
    pub functions: Vec<FunctionEntry>,
    /// All error-code discriminants.
    pub errors: Vec<ErrorEntry>,
    /// Storage key variant names (the enum discriminant tag strings).
    pub storage_keys: Vec<String>,
}

// ── Storage helpers ───────────────────────────────────────────────────────────

/// Persist `schema` in instance storage so the *next* upgrade can validate
/// against it.  Call once during `initialize` and again after every successful
/// `validate_upgrade`.
pub fn store_schema(env: &Env, schema: &ContractSchema) {
    env.storage()
        .instance()
        .set(&DataKey::ContractSchema, schema);
}

/// Load the current on-chain schema.  Returns `None` if the contract was
/// deployed before schema tracking was introduced.
pub fn load_schema(env: &Env) -> Option<ContractSchema> {
    env.storage().instance().get(&DataKey::ContractSchema)
}

// ── Pure compatibility check (testable without WASM swap) ────────────────────

/// Validate that `new_schema` is backward-compatible with `current`.
///
/// This is the pure, side-effect-free part of upgrade validation.  It is
/// separated from the WASM swap so it can be unit-tested without a real
/// Soroban deployer.
///
/// # Errors
///
/// | Error | Condition |
/// |---|---|
/// | `UpgradeSchemaVersionNotGreater` | `new.version <= current.version` |
/// | `UpgradeMissingFunction` | A function in `current` is absent from `new` |
/// | `UpgradeFunctionSignatureChanged` | A function's signature differs |
/// | `UpgradeMissingErrorCode` | An error in `current` is absent from `new` |
/// | `UpgradeErrorCodeChanged` | An error's discriminant changed |
/// | `UpgradeMissingStorageKey` | A storage key in `current` is absent from `new` |
pub fn check_schema_compatibility(
    current: &ContractSchema,
    new_schema: &ContractSchema,
) -> Result<(), ContractError> {
    // 1. Version must advance.
    if new_schema.version <= current.version {
        return Err(ContractError::UpgradeSchemaVersionNotGreater);
    }

    // 2. No function removed or signature changed.
    for i in 0..current.functions.len() {
        let cur_fn = current.functions.get(i).unwrap();
        match find_function(&new_schema.functions, &cur_fn.name) {
            None => return Err(ContractError::UpgradeMissingFunction),
            Some(new_fn) => {
                if new_fn.signature != cur_fn.signature {
                    return Err(ContractError::UpgradeFunctionSignatureChanged);
                }
            }
        }
    }

    // 3. No error code removed or renumbered.
    for i in 0..current.errors.len() {
        let cur_err = current.errors.get(i).unwrap();
        match find_error(&new_schema.errors, &cur_err.name) {
            None => return Err(ContractError::UpgradeMissingErrorCode),
            Some(new_err) => {
                if new_err.code != cur_err.code {
                    return Err(ContractError::UpgradeErrorCodeChanged);
                }
            }
        }
    }

    // 4. No storage key removed.
    for i in 0..current.storage_keys.len() {
        let cur_key = current.storage_keys.get(i).unwrap();
        if !contains_string(&new_schema.storage_keys, &cur_key) {
            return Err(ContractError::UpgradeMissingStorageKey);
        }
    }

    Ok(())
}

// ── Full upgrade entry point (called from the contract impl) ─────────────────

/// Validate `new_schema` against the stored schema, persist the new schema,
/// then swap the WASM.
///
/// If no schema is stored yet (contract deployed before schema tracking was
/// introduced), the new schema is accepted unconditionally and persisted.
pub fn validate_upgrade(
    env: &Env,
    new_wasm_hash: BytesN<32>,
    new_schema: ContractSchema,
) -> Result<(), ContractError> {
    match load_schema(env) {
        None => {
            // First upgrade after schema tracking introduced — accept and seed.
            store_schema(env, &new_schema);
            env.deployer().update_current_contract_wasm(new_wasm_hash);
            Ok(())
        }
        Some(current) => {
            check_schema_compatibility(&current, &new_schema)?;
            store_schema(env, &new_schema);
            env.deployer().update_current_contract_wasm(new_wasm_hash);
            Ok(())
        }
    }
}

// ── Private search helpers ────────────────────────────────────────────────────

fn find_function(haystack: &Vec<FunctionEntry>, name: &String) -> Option<FunctionEntry> {
    for i in 0..haystack.len() {
        let entry = haystack.get(i).unwrap();
        if &entry.name == name {
            return Some(entry);
        }
    }
    None
}

fn find_error(haystack: &Vec<ErrorEntry>, name: &String) -> Option<ErrorEntry> {
    for i in 0..haystack.len() {
        let entry = haystack.get(i).unwrap();
        if &entry.name == name {
            return Some(entry);
        }
    }
    None
}

fn contains_string(haystack: &Vec<String>, needle: &String) -> bool {
    for i in 0..haystack.len() {
        if &haystack.get(i).unwrap() == needle {
            return true;
        }
    }
    false
}

// ── Schema builder ────────────────────────────────────────────────────────────

/// Build the canonical v1 schema for this contract.
///
/// Called once during `initialize` to seed the on-chain manifest, and in tests
/// to construct valid / mutated schemas.  Keep in sync with the actual exported
/// interface.
pub fn build_v1_schema(env: &Env) -> ContractSchema {
    let mut functions: Vec<FunctionEntry> = Vec::new(env);

    macro_rules! f {
        ($name:expr, $sig:expr) => {
            functions.push_back(FunctionEntry {
                name: String::from_str(env, $name),
                signature: String::from_str(env, $sig),
            })
        };
    }

    f!("initialize",               "initialize(ip_registry:Address)->()");
    f!("set_admin",                "set_admin(new_admin:Address)->()");
    f!("pause",                    "pause(caller:Address)->()");
    f!("unpause",                  "unpause(caller:Address)->()");
    f!("upgrade",                  "upgrade(new_wasm_hash:BytesN<32>)->()");
    f!("validate_upgrade",         "validate_upgrade(new_wasm_hash:BytesN<32>,new_schema:ContractSchema)->Result<(),ContractError>");
    f!("initiate_swap",            "initiate_swap(token:Address,ip_id:u64,seller:Address,price:i128,buyer:Address,required_approvals:u32)->u64");
    f!("accept_swap",              "accept_swap(swap_id:u64)->()");
    f!("reveal_key",               "reveal_key(swap_id:u64,caller:Address,secret:BytesN<32>,blinding_factor:BytesN<32>)->()");
    f!("cancel_swap",              "cancel_swap(swap_id:u64,canceller:Address)->()");
    f!("cancel_pending_swap",      "cancel_pending_swap(swap_id:u64,caller:Address)->()");
    f!("cancel_expired_swap",      "cancel_expired_swap(swap_id:u64,caller:Address)->()");
    f!("raise_dispute",            "raise_dispute(swap_id:u64)->()");
    f!("resolve_dispute",          "resolve_dispute(swap_id:u64,caller:Address,refunded:bool)->()");
    f!("auto_resolve_dispute",     "auto_resolve_dispute(swap_id:u64)->()");
    f!("extend_swap_expiry",       "extend_swap_expiry(swap_id:u64,new_expiry:u64)->()");
    f!("approve_swap",             "approve_swap(swap_id:u64,approver:Address)->()");
    f!("get_swap",                 "get_swap(swap_id:u64)->Option<SwapRecord>");
    f!("get_swaps_by_seller",      "get_swaps_by_seller(seller:Address)->Option<Vec<u64>>");
    f!("get_swaps_by_buyer",       "get_swaps_by_buyer(buyer:Address)->Option<Vec<u64>>");
    f!("get_swaps_by_ip",          "get_swaps_by_ip(ip_id:u64)->Option<Vec<u64>>");
    f!("swap_count",               "swap_count()->u64");
    f!("get_swap_history",         "get_swap_history(swap_id:u64)->Vec<SwapHistoryEntry>");
    f!("get_cancellation_reason",  "get_cancellation_reason(swap_id:u64)->Option<Bytes>");
    f!("get_protocol_config",      "get_protocol_config()->ProtocolConfig");
    f!("admin_set_protocol_config","admin_set_protocol_config(protocol_fee_bps:u32,treasury:Address,dispute_window_seconds:u64,dispute_resolution_timeout_seconds:u64)->()");

    let mut errors: Vec<ErrorEntry> = Vec::new(env);

    macro_rules! e {
        ($name:expr, $code:expr) => {
            errors.push_back(ErrorEntry {
                name: String::from_str(env, $name),
                code: $code,
            })
        };
    }

    e!("SwapNotFound",                          1);
    e!("InvalidKey",                            2);
    e!("PriceMustBeGreaterThanZero",            3);
    e!("SellerIsNotTheIPOwner",                 4);
    e!("ActiveSwapAlreadyExistsForThisIpId",    5);
    e!("SwapNotPending",                        6);
    e!("OnlyTheSellerCanRevealTheKey",          7);
    e!("SwapNotAccepted",                       8);
    e!("OnlyTheSellerOrBuyerCanCancel",         9);
    e!("OnlyPendingSwapsCanBeCancelledThisWay", 10);
    e!("SwapNotInAcceptedState",                11);
    e!("OnlyTheBuyerCanCancelAnExpiredSwap",    12);
    e!("SwapHasNotExpiredYet",                  13);
    e!("IpIsRevoked",                           14);
    e!("UnauthorizedUpgrade",                   15);
    e!("InvalidFeeBps",                         16);
    e!("DisputeWindowExpired",                  17);
    e!("OnlyBuyerCanDispute",                   18);
    e!("SwapNotDisputed",                       19);
    e!("OnlyAdminCanResolve",                   20);
    e!("ContractPaused",                        21);
    e!("AlreadyInitialized",                    22);
    e!("Unauthorized",                          23);
    e!("NotInitialized",                        24);
    e!("PendingSwapNotExpired",                 25);
    e!("NewExpiryNotGreater",                   26);
    e!("InsufficientApprovals",                 27);
    e!("AlreadyApproved",                       28);
    e!("UpgradeSchemaVersionNotGreater",        29);
    e!("UpgradeMissingFunction",                30);
    e!("UpgradeFunctionSignatureChanged",       31);
    e!("UpgradeMissingErrorCode",               32);
    e!("UpgradeErrorCodeChanged",               33);
    e!("UpgradeMissingStorageKey",              34);

    let mut storage_keys: Vec<String> = Vec::new(env);

    macro_rules! k {
        ($name:expr) => {
            storage_keys.push_back(String::from_str(env, $name))
        };
    }

    k!("Swap");
    k!("NextId");
    k!("IpRegistry");
    k!("ActiveSwap");
    k!("SellerSwaps");
    k!("BuyerSwaps");
    k!("Admin");
    k!("ProtocolConfig");
    k!("Paused");
    k!("IpSwaps");
    k!("SwapHistory");
    k!("SwapApprovals");
    k!("CancelReason");
    k!("MultiCurrencyConfig");
    k!("SupportedTokens");
    k!("ContractSchema");

    ContractSchema {
        version: 1,
        functions,
        errors,
        storage_keys,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────
//
// All tests call `check_schema_compatibility` directly — the pure function that
// does NOT invoke `env.deployer().update_current_contract_wasm`.  This avoids
// the need for a real Soroban deployer in unit tests.

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn bump_version(s: &ContractSchema) -> ContractSchema {
        ContractSchema {
            version: s.version + 1,
            functions: s.functions.clone(),
            errors: s.errors.clone(),
            storage_keys: s.storage_keys.clone(),
        }
    }

    // ── 1. Valid upgrade passes ───────────────────────────────────────────────

    /// Identical schema with bumped version must pass all checks.
    #[test]
    fn test_valid_upgrade_passes() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let v2 = bump_version(&v1);
        assert_eq!(check_schema_compatibility(&v1, &v2), Ok(()));
    }

    /// Adding a new function is an additive (non-breaking) change.
    #[test]
    fn test_additive_function_passes() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let mut v2 = bump_version(&v1);
        v2.functions.push_back(FunctionEntry {
            name: String::from_str(&env, "new_query"),
            signature: String::from_str(&env, "new_query(id:u64)->bool"),
        });
        assert_eq!(check_schema_compatibility(&v1, &v2), Ok(()));
    }

    /// Adding a new error code is an additive (non-breaking) change.
    #[test]
    fn test_additive_error_code_passes() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let mut v2 = bump_version(&v1);
        v2.errors.push_back(ErrorEntry {
            name: String::from_str(&env, "NewError"),
            code: 99,
        });
        assert_eq!(check_schema_compatibility(&v1, &v2), Ok(()));
    }

    /// Adding a new storage key is an additive (non-breaking) change.
    #[test]
    fn test_additive_storage_key_passes() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let mut v2 = bump_version(&v1);
        v2.storage_keys.push_back(String::from_str(&env, "NewIndex"));
        assert_eq!(check_schema_compatibility(&v1, &v2), Ok(()));
    }

    // ── 2. Version gate ───────────────────────────────────────────────────────

    /// Same version must be rejected.
    #[test]
    fn test_same_version_rejected() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        assert_eq!(
            check_schema_compatibility(&v1, &v1.clone()),
            Err(ContractError::UpgradeSchemaVersionNotGreater)
        );
    }

    /// Lower version must be rejected.
    #[test]
    fn test_lower_version_rejected() {
        let env = Env::default();
        let mut v1 = build_v1_schema(&env);
        v1.version = 5;
        let mut bad = v1.clone();
        bad.version = 3;
        assert_eq!(
            check_schema_compatibility(&v1, &bad),
            Err(ContractError::UpgradeSchemaVersionNotGreater)
        );
    }

    // ── 3. Missing function fails ─────────────────────────────────────────────

    /// Removing a function must be rejected.
    #[test]
    fn test_missing_function_fails() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let mut v2 = bump_version(&v1);

        // Drop "cancel_swap" from v2.
        let mut trimmed: Vec<FunctionEntry> = Vec::new(&env);
        for i in 0..v2.functions.len() {
            let f = v2.functions.get(i).unwrap();
            if f.name != String::from_str(&env, "cancel_swap") {
                trimmed.push_back(f);
            }
        }
        v2.functions = trimmed;

        assert_eq!(
            check_schema_compatibility(&v1, &v2),
            Err(ContractError::UpgradeMissingFunction)
        );
    }

    /// Changing a function's signature must be rejected.
    #[test]
    fn test_function_signature_change_fails() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let mut v2 = bump_version(&v1);

        let mut patched: Vec<FunctionEntry> = Vec::new(&env);
        for i in 0..v2.functions.len() {
            let mut f = v2.functions.get(i).unwrap();
            if f.name == String::from_str(&env, "get_swap") {
                // Change return type — breaking change.
                f.signature = String::from_str(&env, "get_swap(swap_id:u64)->SwapRecord");
            }
            patched.push_back(f);
        }
        v2.functions = patched;

        assert_eq!(
            check_schema_compatibility(&v1, &v2),
            Err(ContractError::UpgradeFunctionSignatureChanged)
        );
    }

    // ── 4. Storage key mismatch fails ─────────────────────────────────────────

    /// Removing a storage key must be rejected.
    #[test]
    fn test_missing_storage_key_fails() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let mut v2 = bump_version(&v1);

        // Drop "SwapHistory".
        let mut trimmed: Vec<String> = Vec::new(&env);
        for i in 0..v2.storage_keys.len() {
            let k = v2.storage_keys.get(i).unwrap();
            if k != String::from_str(&env, "SwapHistory") {
                trimmed.push_back(k);
            }
        }
        v2.storage_keys = trimmed;

        assert_eq!(
            check_schema_compatibility(&v1, &v2),
            Err(ContractError::UpgradeMissingStorageKey)
        );
    }

    // ── 5. Error code change fails ────────────────────────────────────────────

    /// Removing an error entry must be rejected.
    #[test]
    fn test_missing_error_code_fails() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let mut v2 = bump_version(&v1);

        // Drop "InvalidKey".
        let mut trimmed: Vec<ErrorEntry> = Vec::new(&env);
        for i in 0..v2.errors.len() {
            let e = v2.errors.get(i).unwrap();
            if e.name != String::from_str(&env, "InvalidKey") {
                trimmed.push_back(e);
            }
        }
        v2.errors = trimmed;

        assert_eq!(
            check_schema_compatibility(&v1, &v2),
            Err(ContractError::UpgradeMissingErrorCode)
        );
    }

    /// Renumbering an error code must be rejected.
    #[test]
    fn test_error_code_renumbered_fails() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        let mut v2 = bump_version(&v1);

        // Change "SwapNotFound" from 1 → 99.
        let mut patched: Vec<ErrorEntry> = Vec::new(&env);
        for i in 0..v2.errors.len() {
            let mut e = v2.errors.get(i).unwrap();
            if e.name == String::from_str(&env, "SwapNotFound") {
                e.code = 99;
            }
            patched.push_back(e);
        }
        v2.errors = patched;

        assert_eq!(
            check_schema_compatibility(&v1, &v2),
            Err(ContractError::UpgradeErrorCodeChanged)
        );
    }

    // ── 6. Schema persistence ─────────────────────────────────────────────────

    /// `store_schema` / `load_schema` round-trip.
    #[test]
    fn test_store_and_load_schema_round_trip() {
        let env = Env::default();
        let v1 = build_v1_schema(&env);
        store_schema(&env, &v1);
        let loaded = load_schema(&env).expect("schema must be present after store");
        assert_eq!(loaded.version, v1.version);
        assert_eq!(loaded.functions.len(), v1.functions.len());
        assert_eq!(loaded.errors.len(), v1.errors.len());
        assert_eq!(loaded.storage_keys.len(), v1.storage_keys.len());
    }

    /// `load_schema` returns `None` when nothing has been stored.
    #[test]
    fn test_load_schema_returns_none_when_absent() {
        let env = Env::default();
        assert!(load_schema(&env).is_none());
    }
}
