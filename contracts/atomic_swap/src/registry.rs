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
        panic_with_error(env, ContractError::SellerIsNotTheIPOwner);
    }

    if record.revoked {
        panic_with_error(env, ContractError::IpIsRevoked);
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
