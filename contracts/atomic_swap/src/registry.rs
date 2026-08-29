//! Local registry helper for the `atomic_swap` contract.
//!
//! This module is **not** a standalone IP registry. All authoritative IP
//! records are stored in the separate `ip_registry` contract. This file is a
//! thin proxy that:
//!
//! * resolves the `ip_registry` contract address from instance storage
//!   (`DataKey::IpRegistry`), and
//! * exposes two guard functions (`ensure_seller_owns_active_ip`,
//!   `verify_commitment`) that cross-call `ip_registry` to validate ownership
//!   and commitment integrity before the swap contract proceeds.
//!
//! Keeping all cross-contract calls here makes the external data-dependency
//! boundary explicit and easy to audit. See `docs/architecture.md` §
//! "registry.rs — Local Registry Helper" for the full design rationale.

use soroban_sdk::{Address, BytesN, Env};

use crate::{utils::panic_with_error, ContractError, DataKey};

pub fn ip_registry(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::IpRegistry)
        .unwrap_or_else(|| panic_with_error(env, ContractError::NotInitialized))
}

pub fn ensure_seller_owns_active_ip(env: &Env, ip_id: u64, seller: &Address) {
    let registry_addr = ip_registry(env);
    let registry = ip_registry::IpRegistryClient::new(env, &registry_addr);
    let record = registry.get_ip(&ip_id);

    if record.owner != *seller {
        panic_with_error(env, ContractError::NotIPOwner);
    }

    if record.revoked {
        panic_with_error(env, ContractError::IpRevoked);
    }
}

pub fn verify_commitment(
    env: &Env,
    ip_id: u64,
    secret: &BytesN<32>,
    blinding_factor: &BytesN<32>,
) -> bool {
    let registry_addr = ip_registry(env);
    let registry = ip_registry::IpRegistryClient::new(env, &registry_addr);
    registry.verify_commitment(&ip_id, secret, blinding_factor)
}
