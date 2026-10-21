//! Common validation helper functions for the Atomic Swap contract.
//!
//! This module provides reusable validation functions to reduce code duplication
//! and ensure consistent error handling across the contract.

use crate::{ContractError, DataKey, SwapRecord, SwapStatus};
use soroban_sdk::{Address, Env, Error};

#[allow(unused_imports)]
#[cfg(test)]
use soroban_sdk::testutils::Ledger;

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
        env.panic_with_error(Error::from_contract_error(ContractError::Paused as u32));
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
            ContractError::PriceTooSmall as u32,
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
            ContractError::OnlySellerReveal as u32,
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
            ContractError::OnlyBuyerCancel as u32,
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
            ContractError::OnlySellerBuyer as u32,
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
        env.panic_with_error(Error::from_contract_error(ContractError::NotExpired as u32));
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
        env.panic_with_error(Error::from_contract_error(ContractError::SwapExists as u32));
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

/// Validates that the treasury address is not a zero or well-known placeholder address.
/// Issue #906: Guard against hardcoded placeholder treasury addresses.
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `treasury` - The treasury address to validate
///
/// # Panics
///
/// Panics with `InvalidTreasuryAddress` error if the address is invalid (zero or placeholder).
pub fn require_valid_treasury_address(env: &Env, treasury: &Address) {
    // The all-zero contract address (strkey form of 32 zero bytes).
    let zero_address = Address::from_str(env, "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4");
    if treasury == &zero_address {
        env.panic_with_error(Error::from_contract_error(
            ContractError::InvalidTreasuryAddress as u32,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env, Vec};

    /// Convenience constructor so every test doesn't repeat all fields.
    fn make_swap(env: &Env, status: SwapStatus, expiry: u64) -> SwapRecord {
        SwapRecord {
            ip_id: 1,
            seller: Address::generate(env),
            buyer: Address::generate(env),
            price: 100,
            token: Address::generate(env),
            status,
            expiry,
            accept_timestamp: 0,
            required_approvals: 0,
            dispute_timestamp: 0,
            referrer: None,
            collateral_amount: 0,
            insurance_premium: 0,
            insurance_enabled: false,
            escrow_agent: None,
            quantity: 1,
            conditions: Vec::new(env),
        }
    }

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
    fn test_require_swap_exists_succeeds_when_present() {
        let env = Env::default();
        let swap = make_swap(&env, SwapStatus::Pending, 0);
        env.storage().persistent().set(&DataKey::Swap(1), &swap);
        let found = require_swap_exists(&env, 1);
        assert_eq!(found.ip_id, 1);
        assert_eq!(found.status, SwapStatus::Pending);
    }

    #[test]
    #[should_panic(expected = "SwapNotFound")]
    fn test_require_swap_exists_panics_when_missing() {
        let env = Env::default();
        require_swap_exists(&env, 42);
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
        let swap = make_swap(&env, SwapStatus::Pending, 0);
        // Should not panic
        require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::NotPending);
    }

    #[test]
    #[should_panic(expected = "SwapNotPending")]
    fn test_require_swap_status_panics_when_not_matching() {
        let env = Env::default();
        let swap = make_swap(&env, SwapStatus::Accepted, 0);
        require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::NotPending);
    }

    #[test]
    fn test_require_seller_succeeds_when_matching() {
        let env = Env::default();
        let seller = Address::generate(&env);
        let mut swap = make_swap(&env, SwapStatus::Pending, 0);
        swap.seller = seller.clone();
        // Should not panic
        require_seller(&env, &seller, &swap);
    }

    #[test]
    #[should_panic(expected = "OnlyTheSellerCanRevealTheKey")]
    fn test_require_seller_panics_when_not_matching() {
        let env = Env::default();
        let not_seller = Address::generate(&env);
        let swap = make_swap(&env, SwapStatus::Pending, 0);
        require_seller(&env, &not_seller, &swap);
    }

    #[test]
    fn test_require_buyer_succeeds_when_matching() {
        let env = Env::default();
        let buyer = Address::generate(&env);
        let mut swap = make_swap(&env, SwapStatus::Pending, 0);
        swap.buyer = buyer.clone();
        // Should not panic
        require_buyer(&env, &buyer, &swap);
    }

    #[test]
    #[should_panic(expected = "OnlyTheBuyerCanCancelAnExpiredSwap")]
    fn test_require_buyer_panics_when_not_matching() {
        let env = Env::default();
        let not_buyer = Address::generate(&env);
        let swap = make_swap(&env, SwapStatus::Pending, 0);
        require_buyer(&env, &not_buyer, &swap);
    }

    #[test]
    fn test_require_seller_or_buyer_succeeds_for_seller() {
        let env = Env::default();
        let seller = Address::generate(&env);
        let mut swap = make_swap(&env, SwapStatus::Pending, 0);
        swap.seller = seller.clone();
        // Should not panic
        require_seller_or_buyer(&env, &seller, &swap);
    }

    #[test]
    fn test_require_seller_or_buyer_succeeds_for_buyer() {
        let env = Env::default();
        let buyer = Address::generate(&env);
        let mut swap = make_swap(&env, SwapStatus::Pending, 0);
        swap.buyer = buyer.clone();
        // Should not panic
        require_seller_or_buyer(&env, &buyer, &swap);
    }

    #[test]
    #[should_panic(expected = "OnlyTheSellerOrBuyerCanCancel")]
    fn test_require_seller_or_buyer_panics_for_neither() {
        let env = Env::default();
        let neither = Address::generate(&env);
        let swap = make_swap(&env, SwapStatus::Pending, 0);
        require_seller_or_buyer(&env, &neither, &swap);
    }

    #[test]
    fn test_require_swap_expired_succeeds_when_expired() {
        let env = Env::default();
        // Advance the ledger past the swap's expiry so the swap counts as expired.
        let swap = make_swap(&env, SwapStatus::Accepted, 0);
        env.ledger().with_mut(|l| l.timestamp = 1);
        // Should not panic
        require_swap_expired(&env, &swap);
    }

    #[test]
    #[should_panic(expected = "SwapHasNotExpiredYet")]
    fn test_require_swap_expired_panics_when_not_expired() {
        let env = Env::default();
        let swap = make_swap(&env, SwapStatus::Accepted, u64::MAX);
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
        env.storage()
            .persistent()
            .set(&DataKey::ActiveSwap(1), &0u64);
        require_no_active_swap(&env, 1);
    }

    #[test]
    fn test_require_admin_succeeds_when_matching() {
        let env = Env::default();
        let admin = Address::generate(&env);
        env.storage().instance().set(&DataKey::Admin, &admin);
        // Should not panic
        require_admin(&env, &admin);
    }

    #[test]
    #[should_panic(expected = "Unauthorized")]
    fn test_require_admin_panics_when_not_admin() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let not_admin = Address::generate(&env);
        env.storage().instance().set(&DataKey::Admin, &admin);
        require_admin(&env, &not_admin);
    }

    #[test]
    #[should_panic(expected = "Unauthorized")]
    fn test_require_admin_panics_when_not_initialized() {
        let env = Env::default();
        let caller = Address::generate(&env);
        require_admin(&env, &caller);
    }

    #[test]
    fn test_require_valid_treasury_address_accepts_valid_address() {
        let env = Env::default();
        let treasury = Address::generate(&env);
        // Should not panic
        require_valid_treasury_address(&env, &treasury);
    }

    #[test]
    #[should_panic(expected = "InvalidTreasuryAddress")]
    fn test_require_valid_treasury_address_rejects_zero_address() {
        let env = Env::default();
        let zero_address = Address::from_contract_id(&env, &soroban_sdk::BytesN::<32>::new());
        require_valid_treasury_address(&env, &zero_address);
    }
}
