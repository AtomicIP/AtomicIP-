//! Common validation helper functions for the IP Registry contract.
//!
//! This module provides reusable validation functions to reduce code duplication
//! and ensure consistent error handling across the contract.

use crate::{ContractError, DataKey, IpRecord};
use soroban_sdk::{symbol_short, Address, BytesN, Env, Error};

/// Retrieves an IP record by ID, panicking if not found.
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `ip_id` - The unique identifier of the IP
///
/// # Returns
///
/// The `IpRecord` if found.
///
/// # Panics
///
/// Panics with `IpNotFound` error if the IP record does not exist.
pub fn require_ip_exists(env: &Env, ip_id: u64) -> IpRecord {
    env.storage()
        .persistent()
        .get(&DataKey::IpRecord(ip_id))
        .unwrap_or_else(|| {
            env.panic_with_error(Error::from_contract_error(ContractError::IpNotFound as u32))
        })
}

/// Validates that the commitment hash is not all zeros.
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `commitment_hash` - The commitment hash to validate
///
/// # Panics
///
/// Panics with `ZeroCommitmentHash` error if the hash is all zeros.
pub fn require_non_zero_commitment(env: &Env, commitment_hash: &BytesN<32>) {
    if commitment_hash == &BytesN::from_array(env, &[0u8; 32]) {
        env.panic_with_error(Error::from_contract_error(
            ContractError::ZeroCommitmentHash as u32,
        ));
    }
}

/// Validates that the commitment hash is not already registered.
/// If already registered, emits a "collision" event with the existing owner's address,
/// then panics with CommitmentAlreadyRegistered.
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `commitment_hash` - The commitment hash to check
///
/// # Panics
///
/// Panics with `CommitmentAlreadyRegistered` error if the hash is already registered.
pub fn require_unique_commitment(env: &Env, commitment_hash: &BytesN<32>) {
    if let Some(existing_owner) = env
        .storage()
        .persistent()
        .get::<DataKey, Address>(&DataKey::CommitmentOwner(commitment_hash.clone()))
    {
        // Emit event so callers can identify the existing owner
        env.events().publish(
            (symbol_short!("collision"), commitment_hash.clone()),
            existing_owner,
        );
        env.panic_with_error(Error::from_contract_error(
            ContractError::CommitmentAlreadyRegistered as u32,
        ));
    }
}

/// Validates that the IP has not been revoked.
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `record` - The IP record to check
///
/// # Panics
///
/// Panics with `IpAlreadyRevoked` error if the IP has been revoked.
pub fn require_not_revoked(env: &Env, record: &IpRecord) {
    if record.revoked {
        env.panic_with_error(Error::from_contract_error(
            ContractError::IpAlreadyRevoked as u32,
        ));
    }
}

/// Validates that the caller is the owner of the IP.
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `caller` - The address of the caller
/// * `record` - The IP record
///
/// # Panics
///
/// Panics with an auth error if caller is not the owner.
#[allow(dead_code)]
pub fn require_owner(env: &Env, caller: &Address, record: &IpRecord) {
    if caller != &record.owner {
        env.panic_with_error(Error::from_contract_error(
            ContractError::Unauthorized as u32,
        ));
    }
}

/// Validates that the caller is the admin.
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `caller` - The address of the caller
///
/// # Panics
///
/// Panics with `UnauthorizedUpgrade` error if caller is not the admin or admin is not initialized.
#[allow(dead_code)]
pub fn require_admin(env: &Env, caller: &Address) {
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| {
            env.panic_with_error(Error::from_contract_error(
                ContractError::UnauthorizedUpgrade as u32,
            ))
        });
    if caller != &admin {
        env.panic_with_error(Error::from_contract_error(
            ContractError::UnauthorizedUpgrade as u32,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn test_require_non_zero_commitment_succeeds_for_non_zero() {
        let env = Env::default();
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        // Should not panic
        require_non_zero_commitment(&env, &hash);
    }

    #[test]
    #[should_panic(expected = "ZeroCommitmentHash")]
    fn test_require_non_zero_commitment_panics_for_zero() {
        let env = Env::default();
        let hash = BytesN::from_array(&env, &[0u8; 32]);
        require_non_zero_commitment(&env, &hash);
    }

    #[test]
    fn test_require_unique_commitment_succeeds_for_new() {
        let env = Env::default();
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        // Should not panic
        require_unique_commitment(&env, &hash);
    }

    #[test]
    #[should_panic(expected = "CommitmentAlreadyRegistered")]
    fn test_require_unique_commitment_panics_for_duplicate() {
        let env = Env::default();
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let owner = Address::generate(&env);
        env.storage()
            .persistent()
            .set(&DataKey::CommitmentOwner(hash.clone()), &owner);
        require_unique_commitment(&env, &hash);
    }

    #[test]
    fn test_require_not_revoked_succeeds_when_not_revoked() {
        let env = Env::default();
        let record = IpRecord {
            ip_id: 1,
            owner: Address::generate(&env),
            commitment_hash: BytesN::from_array(&env, &[1u8; 32]),
            timestamp: 0,
            revoked: false,
            expiry_timestamp: 0,
            metadata: soroban_sdk::Bytes::new(&env),
        };
        // Should not panic
        require_not_revoked(&env, &record);
    }

    #[test]
    #[should_panic(expected = "IpAlreadyRevoked")]
    fn test_require_not_revoked_panics_when_revoked() {
        let env = Env::default();
        let record = IpRecord {
            ip_id: 1,
            owner: Address::generate(&env),
            commitment_hash: BytesN::from_array(&env, &[1u8; 32]),
            timestamp: 0,
            revoked: true,
            expiry_timestamp: 0,
            metadata: soroban_sdk::Bytes::new(&env),
        };
        require_not_revoked(&env, &record);
    }

    #[test]
    fn test_require_owner_succeeds_when_matching() {
        let env = Env::default();
        let owner = Address::generate(&env);
        let record = IpRecord {
            ip_id: 1,
            owner: owner.clone(),
            commitment_hash: BytesN::from_array(&env, &[1u8; 32]),
            timestamp: 0,
            revoked: false,
            expiry_timestamp: 0,
            metadata: soroban_sdk::Bytes::new(&env),
        };
        // Should not panic
        require_owner(&env, &owner, &record);
    }

    #[test]
    #[should_panic(expected = "Unauthorized")]
    fn test_require_owner_panics_when_not_matching() {
        let env = Env::default();
        let owner = Address::generate(&env);
        let not_owner = Address::generate(&env);
        let record = IpRecord {
            ip_id: 1,
            owner: owner.clone(),
            commitment_hash: BytesN::from_array(&env, &[1u8; 32]),
            timestamp: 0,
            revoked: false,
            expiry_timestamp: 0,
            metadata: soroban_sdk::Bytes::new(&env),
        };
        require_owner(&env, &not_owner, &record);
    }
}
