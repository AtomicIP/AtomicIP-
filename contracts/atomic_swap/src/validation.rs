//! Common validation helper functions for the Atomic Swap contract.
//!
//! This module provides reusable validation functions to reduce code duplication
//! and ensure consistent error handling across the contract.

use soroban_sdk::{Address, Env, Error};
use crate::{ContractError, DataKey, SwapRecord, SwapStatus};

/// Validates that the contract is not paused.
///
/// # Arguments
///
/// * `env` - The Soroban environment
///
/// # Panics
///
/// Panics with `ContractPaused` error if the contract is paused.
pub fn require_not_paused(env: &Env) {
    if env
        .storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        env.panic_with_error(Error::from_contract_error(
            ContractError::ContractPaused as u32,
        ));
    }
}

/// Retrieves a swap record by ID, panicking if not found.
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `swap_id` - The unique identifier of the swap
///
/// # Returns
///
/// The `SwapRecord` if found.
///
/// # Panics
///
/// Panics with `SwapNotFound` error if the swap does not exist.
pub fn require_swap_exists(env: &Env, swap_id: u64) -> SwapRecord {
    env.storage()
        .persistent()
        .get(&DataKey::Swap(swap_id))
        .unwrap_or_else(|| {
            env.panic_with_error(Error::from_contract_error(
                ContractError::SwapNotFound as u32,
            ))
        })
}

/// Validates that a swap is in the expected status.
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `swap` - The swap record to validate
/// * `expected_status` - The expected swap status
/// * `error` - The error to panic with if status doesn't match
///
/// # Panics
///
/// Panics with the provided error if the swap status doesn't match the expected status.
pub fn require_swap_status(
    env: &Env,
    swap: &SwapRecord,
    expected_status: SwapStatus,
    error: ContractError,
) {
    if swap.status != expected_status {
        env.panic_with_error(Error::from_contract_error(error as u32));
    }
}

/// Validates that the price is greater than zero.
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `price` - The price to validate
///
/// # Panics
///
/// Panics with `PriceMustBeGreaterThanZero` error if price is zero or negative.
pub fn require_positive_price(env: &Env, price: i128) {
    if price <= 0 {
        env.panic_with_error(Error::from_contract_error(
            ContractError::PriceMustBeGreaterThanZero as u32,
        ));
    }
}

/// Validates that the caller is the seller of the swap.
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `caller` - The address of the caller
/// * `swap` - The swap record
///
/// # Panics
///
/// Panics with `OnlyTheSellerCanRevealTheKey` error if caller is not the seller.
pub fn require_seller(env: &Env, caller: &Address, swap: &SwapRecord) {
    if caller != &swap.seller {
        env.panic_with_error(Error::from_contract_error(
            ContractError::OnlyTheSellerCanRevealTheKey as u32,
        ));
    }
}

/// Validates that the caller is the buyer of the swap.
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `caller` - The address of the caller
/// * `swap` - The swap record
///
/// # Panics
///
/// Panics with `OnlyTheBuyerCanCancelAnExpiredSwap` error if caller is not the buyer.
pub fn require_buyer(env: &Env, caller: &Address, swap: &SwapRecord) {
    if caller != &swap.buyer {
        env.panic_with_error(Error::from_contract_error(
            ContractError::OnlyTheBuyerCanCancelAnExpiredSwap as u32,
        ));
    }
}

/// Validates that the caller is either the seller or buyer of the swap.
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `caller` - The address of the caller
/// * `swap` - The swap record
///
/// # Panics
///
/// Panics with `OnlyTheSellerOrBuyerCanCancel` error if caller is neither seller nor buyer.
pub fn require_seller_or_buyer(env: &Env, caller: &Address, swap: &SwapRecord) {
    if caller != &swap.seller && caller != &swap.buyer {
        env.panic_with_error(Error::from_contract_error(
            ContractError::OnlyTheSellerOrBuyerCanCancel as u32,
        ));
    }
}

/// Validates that the swap has expired.
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `swap` - The swap record
///
/// # Panics
///
/// Panics with `SwapHasNotExpiredYet` error if the swap has not expired.
pub fn require_swap_expired(env: &Env, swap: &SwapRecord) {
    if env.ledger().timestamp() <= swap.expiry {
        env.panic_with_error(Error::from_contract_error(
            ContractError::SwapHasNotExpiredYet as u32,
        ));
    }
}

/// Validates that there is no active swap for the given IP ID.
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `ip_id` - The IP ID to check
///
/// # Panics
///
/// Panics with `ActiveSwapAlreadyExistsForThisIpId` error if an active swap exists.
pub fn require_no_active_swap(env: &Env, ip_id: u64) {
    if env.storage().persistent().has(&DataKey::ActiveSwap(ip_id)) {
        env.panic_with_error(Error::from_contract_error(
            ContractError::ActiveSwapAlreadyExistsForThisIpId as u32,
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
/// Panics with `Unauthorized` error if caller is not the admin or admin is not initialized.
pub fn require_admin(env: &Env, caller: &Address) {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| {
            env.panic_with_error(Error::from_contract_error(
                ContractError::Unauthorized as u32,
            ))
        });
    if caller != &admin {
        env.panic_with_error(Error::from_contract_error(
            ContractError::Unauthorized as u32,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn test_require_not_paused_succeeds_when_not_paused() {
        let env = Env::default();
        // Should not panic
        require_not_paused(&env);
    }

    #[test]
    #[should_panic(expected = "ContractPaused")]
    fn test_require_not_paused_panics_when_paused() {
        let env = Env::default();
        env.storage().instance().set(&DataKey::Paused, &true);
        require_not_paused(&env);
    }

    #[test]
    fn test_require_positive_price_succeeds_for_positive() {
        let env = Env::default();
        // Should not panic
        require_positive_price(&env, 100);
    }

    #[test]
    #[should_panic(expected = "PriceMustBeGreaterThanZero")]
    fn test_require_positive_price_panics_for_zero() {
        let env = Env::default();
        require_positive_price(&env, 0);
    }

    #[test]
    #[should_panic(expected = "PriceMustBeGreaterThanZero")]
    fn test_require_positive_price_panics_for_negative() {
        let env = Env::default();
        require_positive_price(&env, -100);
    }

    #[test]
    fn test_require_swap_status_succeeds_when_matching() {
        let env = Env::default();
        let swap = SwapRecord {
            ip_id: 1,
            seller: Address::generate(&env),
            buyer: Address::generate(&env),
            price: 100,
            token: Address::generate(&env),
            status: SwapStatus::Pending,
            expiry: 0,
            accept_timestamp: 0,
        };
        // Should not panic
        require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::SwapNotPending);
    }

    #[test]
    #[should_panic(expected = "SwapNotPending")]
    fn test_require_swap_status_panics_when_not_matching() {
        let env = Env::default();
        let swap = SwapRecord {
            ip_id: 1,
            seller: Address::generate(&env),
            buyer: Address::generate(&env),
            price: 100,
            token: Address::generate(&env),
            status: SwapStatus::Accepted,
            expiry: 0,
            accept_timestamp: 0,
        };
        require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::SwapNotPending);
    }

    #[test]
    fn test_require_seller_succeeds_when_matching() {
        let env = Env::default();
        let seller = Address::generate(&env);
        let swap = SwapRecord {
            ip_id: 1,
            seller: seller.clone(),
            buyer: Address::generate(&env),
            price: 100,
            token: Address::generate(&env),
            status: SwapStatus::Pending,
            expiry: 0,
            accept_timestamp: 0,
        };
        // Should not panic
        require_seller(&env, &seller, &swap);
    }

    #[test]
    #[should_panic(expected = "OnlyTheSellerCanRevealTheKey")]
    fn test_require_seller_panics_when_not_matching() {
        let env = Env::default();
        let seller = Address::generate(&env);
        let not_seller = Address::generate(&env);
        let swap = SwapRecord {
            ip_id: 1,
            seller: seller.clone(),
            buyer: Address::generate(&env),
            price: 100,
            token: Address::generate(&env),
            status: SwapStatus::Pending,
            expiry: 0,
            accept_timestamp: 0,
        };
        require_seller(&env, &not_seller, &swap);
    }

    #[test]
    fn test_require_buyer_succeeds_when_matching() {
        let env = Env::default();
        let buyer = Address::generate(&env);
        let swap = SwapRecord {
            ip_id: 1,
            seller: Address::generate(&env),
            buyer: buyer.clone(),
            price: 100,
            token: Address::generate(&env),
            status: SwapStatus::Pending,
            expiry: 0,
            accept_timestamp: 0,
        };
        // Should not panic
        require_buyer(&env, &buyer, &swap);
    }

    #[test]
    #[should_panic(expected = "OnlyTheBuyerCanCancelAnExpiredSwap")]
    fn test_require_buyer_panics_when_not_matching() {
        let env = Env::default();
        let buyer = Address::generate(&env);
        let not_buyer = Address::generate(&env);
        let swap = SwapRecord {
            ip_id: 1,
            seller: Address::generate(&env),
            buyer: buyer.clone(),
            price: 100,
            token: Address::generate(&env),
            status: SwapStatus::Pending,
            expiry: 0,
            accept_timestamp: 0,
        };
        require_buyer(&env, &not_buyer, &swap);
    }

    #[test]
    fn test_require_seller_or_buyer_succeeds_for_seller() {
        let env = Env::default();
        let seller = Address::generate(&env);
        let swap = SwapRecord {
            ip_id: 1,
            seller: seller.clone(),
            buyer: Address::generate(&env),
            price: 100,
            token: Address::generate(&env),
            status: SwapStatus::Pending,
            expiry: 0,
            accept_timestamp: 0,
        };
        // Should not panic
        require_seller_or_buyer(&env, &seller, &swap);
    }

    #[test]
    fn test_require_seller_or_buyer_succeeds_for_buyer() {
        let env = Env::default();
        let buyer = Address::generate(&env);
        let swap = SwapRecord {
            ip_id: 1,
            seller: Address::generate(&env),
            buyer: buyer.clone(),
            price: 100,
            token: Address::generate(&env),
            status: SwapStatus::Pending,
            expiry: 0,
            accept_timestamp: 0,
        };
        // Should not panic
        require_seller_or_buyer(&env, &buyer, &swap);
    }

    #[test]
    #[should_panic(expected = "OnlyTheSellerOrBuyerCanCancel")]
    fn test_require_seller_or_buyer_panics_for_neither() {
        let env = Env::default();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let neither = Address::generate(&env);
        let swap = SwapRecord {
            ip_id: 1,
            seller: seller.clone(),
            buyer: buyer.clone(),
            price: 100,
            token: Address::generate(&env),
            status: SwapStatus::Pending,
            expiry: 0,
            accept_timestamp: 0,
        };
        require_seller_or_buyer(&env, &neither, &swap);
    }

    #[test]
    fn test_require_swap_expired_succeeds_when_expired() {
        let env = Env::default();
        let swap = SwapRecord {
            ip_id: 1,
            seller: Address::generate(&env),
            buyer: Address::generate(&env),
            price: 100,
            token: Address::generate(&env),
            status: SwapStatus::Accepted,
            expiry: 0, // Expired (timestamp is > 0)
            accept_timestamp: 0,
        };
        // Should not panic
        require_swap_expired(&env, &swap);
    }

    #[test]
    #[should_panic(expected = "SwapHasNotExpiredYet")]
    fn test_require_swap_expired_panics_when_not_expired() {
        let env = Env::default();
        let swap = SwapRecord {
            ip_id: 1,
            seller: Address::generate(&env),
            buyer: Address::generate(&env),
            price: 100,
            token: Address::generate(&env),
            status: SwapStatus::Accepted,
            expiry: u64::MAX, // Far in the future
            accept_timestamp: 0,
        };
        require_swap_expired(&env, &swap);
    }

    #[test]
    fn test_require_no_active_swap_succeeds_when_no_active_swap() {
        let env = Env::default();
        // Should not panic
        require_no_active_swap(&env, 1);
    }

    #[test]
    #[should_panic(expected = "ActiveSwapAlreadyExistsForThisIpId")]
    fn test_require_no_active_swap_panics_when_active_swap_exists() {
        let env = Env::default();
        env.storage().persistent().set(&DataKey::ActiveSwap(1), &0u64);
        require_no_active_swap(&env, 1);
    }
}
