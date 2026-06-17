//! Price Oracle Integration for Atomic Swap
//!
//! Provides dynamic pricing by querying an on-chain price oracle contract.
//! The oracle contract must implement the `get_price(token: Address) -> i128`
//! interface (returns price in the same unit as swap prices, i.e. stroops).
//!
//! # Design
//! - Admin sets the oracle contract address via `set_oracle`.
//! - `initiate_swap_with_oracle_price` fetches the current price from the oracle
//!   and validates it falls within an optional `[min_price, max_price]` band
//!   before creating the swap.
//! - The oracle address is stored under `DataKey::OracleConfig`.

use soroban_sdk::{contracttype, symbol_short, Address, Env, IntoVal, Val};

use crate::{ContractError, DataKey, LEDGER_BUMP};

// ── Oracle Config ─────────────────────────────────────────────────────────────

/// Configuration for the price oracle.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OracleConfig {
    /// Address of the oracle contract.
    pub oracle_address: Address,
    /// Whether oracle-based pricing is enabled.
    pub enabled: bool,
}

// ── Oracle Event ──────────────────────────────────────────────────────────────

/// Emitted when the oracle config is updated by admin.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OracleConfigSetEvent {
    pub oracle_address: Address,
    pub enabled: bool,
}

/// Emitted when a swap is initiated using an oracle-derived price.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OraclePriceUsedEvent {
    pub swap_id: u64,
    pub oracle_price: i128,
}

// ── Storage helpers ───────────────────────────────────────────────────────────

pub fn store_oracle_config(env: &Env, config: &OracleConfig) {
    env.storage()
        .persistent()
        .set(&DataKey::OracleConfig, config);
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::OracleConfig, LEDGER_BUMP, LEDGER_BUMP);
}

pub fn load_oracle_config(env: &Env) -> Option<OracleConfig> {
    env.storage().persistent().get(&DataKey::OracleConfig)
}

// ── Oracle client ─────────────────────────────────────────────────────────────

/// Calls `get_price(token)` on the configured oracle contract.
/// Returns the price in stroops (i128).
///
/// # Errors
/// Panics with `OracleNotConfigured` if no oracle is set or it is disabled.
/// Panics with `OraclePriceInvalid` if the returned price is ≤ 0.
pub fn fetch_oracle_price(env: &Env, token: &Address) -> i128 {
    let config = load_oracle_config(env).unwrap_or_else(|| {
        env.panic_with_error(soroban_sdk::Error::from_contract_error(
            ContractError::OracleNotConfigured as u32,
        ))
    });

    if !config.enabled {
        env.panic_with_error(soroban_sdk::Error::from_contract_error(
            ContractError::OracleNotConfigured as u32,
        ));
    }

    // Cross-contract call: oracle must expose `get_price(token: Address) -> i128`
    let mut args: soroban_sdk::Vec<Val> = soroban_sdk::Vec::new(env);
    args.push_back(token.into_val(env));
    let price: i128 = env.invoke_contract(
        &config.oracle_address,
        &symbol_short!("get_price"),
        args,
    );

    if price <= 0 {
        env.panic_with_error(soroban_sdk::Error::from_contract_error(
            ContractError::OraclePriceInvalid as u32,
        ));
    }

    price
}

/// Validates that `price` falls within `[min_price, max_price]` if bounds are set.
/// A value of `0` for either bound means "no bound".
pub fn validate_price_bounds(env: &Env, price: i128, min_price: i128, max_price: i128) {
    if min_price > 0 && price < min_price {
        env.panic_with_error(soroban_sdk::Error::from_contract_error(
            ContractError::OraclePriceBelowMin as u32,
        ));
    }
    if max_price > 0 && price > max_price {
        env.panic_with_error(soroban_sdk::Error::from_contract_error(
            ContractError::OraclePriceAboveMax as u32,
        ));
    }
}
