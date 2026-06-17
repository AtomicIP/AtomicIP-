//! Common validation helper functions for the IP Registry contract.
//!
//! This module provides reusable validation functions to reduce code duplication
//! and ensure consistent error handling across the contract.

use crate::{ContractError, DataKey, IpRecord};
use soroban_sdk::{symbol_short, Address, Bytes, BytesN, Env, Error};

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

/// Validates that the commitment hash meets the proof-of-work difficulty requirement.
/// The hash must have at least `difficulty` leading zero bits.
///
/// # Panics
///
/// Panics with `InsufficientPoW` if the hash does not meet the difficulty.
pub fn require_pow(env: &Env, commitment_hash: &BytesN<32>, difficulty: u32) {
    if difficulty == 0 {
        return;
    }
    let bytes = commitment_hash.to_array();
    let mut remaining = difficulty;
    for byte in bytes.iter() {
        if remaining == 0 {
            break;
        }
        let bits = if remaining >= 8 { 8 } else { remaining };
        let mask: u8 = !((1u8 << (8 - bits)).wrapping_sub(1));
        if byte & mask != 0 {
            env.panic_with_error(Error::from_contract_error(
                ContractError::InsufficientPoW as u32,
            ));
        }
        remaining = remaining.saturating_sub(8);
    }
}

/// Validates that a category hash is non-zero.
///
/// # Panics
///
/// Panics with `InvalidCategoryHash` if the hash is all zeros.
pub fn require_valid_category_hash(env: &Env, category_hash: &BytesN<32>) {
    if category_hash == &BytesN::from_array(env, &[0u8; 32]) {
        env.panic_with_error(Error::from_contract_error(
            ContractError::InvalidCategoryHash as u32,
        ));
    }
}

/// Validates a UTF-8 category path string for max depth and path traversal.
///
/// Rules:
/// - Max depth is `MAX_CATEGORY_DEPTH` segments separated by `/`.
/// - No empty segments (e.g., `//`, leading/trailing `/`).
/// - No `..` path traversal segments.
/// - Path length must be between 1 and 512 bytes.
///
/// # Returns
///
/// `sha256(path)` to use as the category_hash for storage.
///
/// # Panics
///
/// Panics with `CategoryDepthExceeded` or `CategoryPathTraversal` on validation failure.
pub fn validate_category_path(env: &Env, path: &Bytes) -> BytesN<32> {
    let len = path.len();
    if len == 0 || len > 512 {
        env.panic_with_error(Error::from_contract_error(
            ContractError::CategoryPathTraversal as u32,
        ));
    }

    let mut depth: u32 = 0;
    let mut seg_start: u32 = 0;

    for i in 0..len {
        if path.get(i).unwrap() == b'/' {
            // Empty segment (leading slash, double slash)
            if i == seg_start {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::CategoryPathTraversal as u32,
                ));
            }
            // Check for ".." segment
            let seg_len = i - seg_start;
            if seg_len == 2
                && path.get(seg_start).unwrap() == b'.'
                && path.get(seg_start + 1).unwrap() == b'.'
            {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::CategoryPathTraversal as u32,
                ));
            }
            depth += 1;
            seg_start = i + 1;
        }
    }

    // Validate last segment
    if seg_start >= len {
        // Trailing slash
        env.panic_with_error(Error::from_contract_error(
            ContractError::CategoryPathTraversal as u32,
        ));
    }
    let last_seg_len = len - seg_start;
    if last_seg_len == 2
        && path.get(seg_start).unwrap() == b'.'
        && path.get(seg_start + 1).unwrap() == b'.'
    {
        env.panic_with_error(Error::from_contract_error(
            ContractError::CategoryPathTraversal as u32,
        ));
    }

    // depth = number of separators, so total segments = depth + 1
    let total_segments = depth + 1;
    if total_segments > crate::MAX_CATEGORY_DEPTH {
        env.panic_with_error(Error::from_contract_error(
            ContractError::CategoryDepthExceeded as u32,
        ));
    }

    env.crypto().sha256(path).into()
}

/// Calculate commitment strength (0-100 scale) based on secret length and PoW difficulty.
/// Strength = min(100, (secret_length * 2) + (pow_difficulty * 3))
#[allow(dead_code)]
pub fn calculate_commitment_strength(secret_length: u32, pow_difficulty: u32) -> u8 {
    let strength = (secret_length * 2).saturating_add(pow_difficulty * 3);
    if strength > 100 {
        100u8
    } else {
        strength as u8
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
    #[ignore]
    fn test_require_non_zero_commitment_panics_for_zero() {
        let env = Env::default();
        let hash = BytesN::from_array(&env, &[0u8; 32]);
        require_non_zero_commitment(&env, &hash);
    }

    #[test]
    #[ignore]
    fn test_require_unique_commitment_succeeds_for_new() {
        let env = Env::default();
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        // Should not panic
        require_unique_commitment(&env, &hash);
    }

    #[test]
    #[should_panic(expected = "CommitmentAlreadyRegistered")]
    #[ignore]
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
            co_owners: soroban_sdk::Vec::new(&env),
            parent_ip_id: None,
            notary_signature: None,
            expiry_timestamp: 0,
            grace_period_seconds: 0,
        };
        // Should not panic
        require_not_revoked(&env, &record);
    }

    #[test]
    #[should_panic(expected = "IpAlreadyRevoked")]
    #[ignore]
    fn test_require_not_revoked_panics_when_revoked() {
        let env = Env::default();
        let record = IpRecord {
            ip_id: 1,
            owner: Address::generate(&env),
            commitment_hash: BytesN::from_array(&env, &[1u8; 32]),
            timestamp: 0,
            revoked: true,
            co_owners: soroban_sdk::Vec::new(&env),
            parent_ip_id: None,
            notary_signature: None,
            expiry_timestamp: 0,
            grace_period_seconds: 0,
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
            co_owners: soroban_sdk::Vec::new(&env),
            parent_ip_id: None,
            notary_signature: None,
            expiry_timestamp: 0,
            grace_period_seconds: 0,
        };
        // Should not panic
        require_owner(&env, &owner, &record);
    }

    #[test]
    #[should_panic(expected = "Unauthorized")]
    #[ignore]
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
            co_owners: soroban_sdk::Vec::new(&env),
            parent_ip_id: None,
            notary_signature: None,
            expiry_timestamp: 0,
            grace_period_seconds: 0,
        };
        require_owner(&env, &not_owner, &record);
    }
}
