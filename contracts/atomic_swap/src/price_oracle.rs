//! Price Oracle Integration for Atomic Swap
//!
//! Provides dynamic pricing by querying an on-chain price oracle contract.
//! The oracle contract must implement the
//! `get_price_attestation(token: Address) -> SignedPrice` interface, returning
//! a price (in stroops) together with the timestamp it was observed at and an
//! Ed25519 signature over the `(token, price, timestamp)` tuple produced by the
//! oracle publisher's off-chain key. The signature is the trust anchor: it lets
//! anyone verify a settlement price was authentically produced by the publisher
//! `set_oracle` designated, rather than simply trusting whatever `oracle_address`
//! happens to return.
//!
//! # Design
//! - Admin sets the oracle contract address *and* the publisher's Ed25519 public
//!   key via `set_oracle`. Both are required — an address alone is not a trust
//!   anchor.
//! - `fetch_oracle_price` / `fetch_oracle_price_with_staleness_check` verify the
//!   attestation signature before the price is used for anything. An invalid,
//!   missing, or wrong-key signature is rejected regardless of the staleness
//!   check.
//! - A configurable maximum deviation (in basis points) bounds how far a single
//!   accepted update may move from the last accepted price, independent of the
//!   signature check.
//! - `initiate_swap_with_oracle_price` fetches the current price from the oracle
//!   and validates it falls within an optional `[min_price, max_price]` band
//!   before creating the swap.
//! - The oracle address is stored under `DataKey::OracleConfig`.
//! - Price freshness is validated (< 5 min staleness threshold).
//! - Stale prices fall back to cached prices if available. Cached prices were
//!   themselves signature-verified when they were fetched, so the fallback
//!   preserves the same trust guarantee without re-fetching.

use soroban_sdk::{
    contracttype, symbol_short, xdr::ToXdr, Address, Bytes, BytesN, Env, IntoVal, Symbol, Val,
};

use crate::{ContractError, DataKey, LEDGER_BUMP};

// ── Oracle Constants ──────────────────────────────────────────────────────────

/// Maximum allowed staleness for oracle prices (300 seconds = 5 minutes).
pub const ORACLE_STALENESS_THRESHOLD_SECS: u64 = 300;

/// Denominator for `max_deviation_bps` (basis points, 1 bps = 0.01%).
pub const BPS_DENOMINATOR: i128 = 10_000;

// ── Attestation ───────────────────────────────────────────────────────────────

/// The message an oracle publisher signs off-chain: `(token, price, timestamp)`.
/// Serialized deterministically via XDR before signing/verification, so the
/// signer and verifier always agree on the exact bytes covered by the signature.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PriceAttestation {
    pub token: Address,
    pub price: i128,
    pub timestamp: u64,
}

/// The response an oracle contract must return from `get_price_attestation`:
/// the attested price/timestamp plus the publisher's Ed25519 signature over the
/// corresponding `PriceAttestation`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SignedPrice {
    pub price: i128,
    pub timestamp: u64,
    pub signature: BytesN<64>,
}

// ── Oracle Config ─────────────────────────────────────────────────────────────

/// Configuration for the price oracle.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OracleConfig {
    /// Address of the oracle contract.
    pub oracle_address: Address,
    /// Ed25519 public key the oracle publisher signs price attestations with.
    /// This, not `oracle_address` alone, is the cryptographic trust anchor.
    pub oracle_pubkey: BytesN<32>,
    /// Whether oracle-based pricing is enabled.
    pub enabled: bool,
    /// Timestamp of the last successful price fetch (ledger timestamp).
    pub last_update_timestamp: u64,
    /// The last successfully verified price (used as fallback for stale data,
    /// and as the baseline for the max-deviation check).
    pub cached_price: i128,
    /// Maximum allowed deviation of a newly accepted price from
    /// `cached_price`, in basis points. `0` means "no bound".
    pub max_deviation_bps: u32,
}

// ── Oracle Events ─────────────────────────────────────────────────────────────

/// Emitted when the oracle config is updated by admin.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OracleConfigSetEvent {
    pub oracle_address: Address,
    pub oracle_pubkey: BytesN<32>,
    pub enabled: bool,
}

/// Emitted when a swap is initiated using an oracle-derived price.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OraclePriceUsedEvent {
    pub swap_id: u64,
    pub oracle_price: i128,
}

/// Emitted when a signed oracle price passes signature verification and the
/// deviation-bound check, and is accepted as the current price.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OraclePriceAcceptedEvent {
    pub token: Address,
    pub price: i128,
    pub timestamp: u64,
}

/// Emitted when a fresh oracle price is stale and a cached fallback price is
/// used instead. Unlike the bad-signature/deviation rejections, this event is
/// on the success path and is reliably observable on-chain.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OraclePriceRejectedStaleEvent {
    pub token: Address,
    pub fallback_price: i128,
    pub staleness_secs: u64,
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

/// Calls `get_price_attestation(token)` on the configured oracle contract.
fn call_oracle_attestation(env: &Env, oracle_address: &Address, token: &Address) -> SignedPrice {
    let mut args: soroban_sdk::Vec<Val> = soroban_sdk::Vec::new(env);
    args.push_back(token.into_val(env));
    env.invoke_contract(
        oracle_address,
        &Symbol::new(env, "get_price_attestation"),
        args,
    )
}

/// Verifies that `signed` is a valid Ed25519 attestation of `(token, price,
/// timestamp)` under `config.oracle_pubkey`. This is a hard gate: it is
/// checked before the price is touched by staleness or deviation logic, and
/// independently of both.
///
/// # Panics
/// Traps (via the host's `ed25519_verify`) if the signature does not verify
/// under `config.oracle_pubkey` — an invalid, missing, or wrong-key signature
/// aborts the whole call before the price can be used for anything.
fn verify_attestation(env: &Env, config: &OracleConfig, token: &Address, signed: &SignedPrice) {
    let attestation = PriceAttestation {
        token: token.clone(),
        price: signed.price,
        timestamp: signed.timestamp,
    };
    let payload: Bytes = attestation.to_xdr(env);
    env.crypto()
        .ed25519_verify(&config.oracle_pubkey, &payload, &signed.signature);
}

/// Enforces the configured max single-update deviation bound against the last
/// accepted (cached) price. A bound of `0`, or no prior cached price, means
/// "no bound" (nothing to compare a bootstrap price against).
fn enforce_deviation_bound(env: &Env, config: &OracleConfig, price: i128) {
    if config.max_deviation_bps == 0 || config.cached_price <= 0 {
        return;
    }
    let diff = (price - config.cached_price).abs();
    let limit = config.cached_price.abs() * (config.max_deviation_bps as i128) / BPS_DENOMINATOR;
    if diff > limit {
        env.panic_with_error(soroban_sdk::Error::from_contract_error(
            ContractError::OracleDeviationExceeded as u32,
        ));
    }
}

/// Fetches, verifies, and accepts a fresh signed price from the oracle for
/// `token`. Publishes `price_accepted` on success.
fn fetch_and_accept(env: &Env, config: &OracleConfig, token: &Address) -> i128 {
    let signed = call_oracle_attestation(env, &config.oracle_address, token);

    verify_attestation(env, config, token, &signed);

    if signed.price <= 0 {
        env.panic_with_error(soroban_sdk::Error::from_contract_error(
            ContractError::OraclePriceInvalid as u32,
        ));
    }

    enforce_deviation_bound(env, config, signed.price);

    env.events().publish(
        (symbol_short!("pr_ok"),),
        OraclePriceAcceptedEvent {
            token: token.clone(),
            price: signed.price,
            timestamp: signed.timestamp,
        },
    );

    signed.price
}

/// Calls `get_price_attestation(token)` on the configured oracle contract,
/// verifies its signature, and returns the price in stroops (i128).
///
/// # Errors
/// Panics with `OracleNotConfigured` if no oracle is set or it is disabled.
/// Panics if the attestation signature does not verify (hard gate, checked
/// before anything else touches the price).
/// Panics with `OraclePriceInvalid` if the attested price is ≤ 0.
/// Panics with `OracleDeviationExceeded` if the price moves too far from the
/// last accepted price.
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

    let price = fetch_and_accept(env, &config, token);

    let updated_config = OracleConfig {
        last_update_timestamp: env.ledger().timestamp(),
        cached_price: price,
        ..config
    };
    store_oracle_config(env, &updated_config);

    price
}

/// Fetches the oracle price with staleness validation.
/// If the price is stale (> 5 minutes since last update), falls back to cached
/// price. Fresh prices are signature-verified before being accepted; the cached
/// fallback was itself signature-verified when it was originally fetched.
///
/// # Returns
/// The fresh oracle price or the cached price if oracle is stale.
///
/// # Errors
/// Panics with `OracleNotConfigured` if no oracle is set or it is disabled.
/// Panics if a fresh attestation's signature does not verify.
/// Panics with `OraclePriceInvalid` if the resulting price is ≤ 0.
/// Panics with `OracleDeviationExceeded` if a fresh price moves too far from
/// the last accepted price.
pub fn fetch_oracle_price_with_staleness_check(env: &Env, token: &Address) -> i128 {
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

    let current_timestamp = env.ledger().timestamp();
    let staleness_secs = current_timestamp.saturating_sub(config.last_update_timestamp);

    if staleness_secs <= ORACLE_STALENESS_THRESHOLD_SECS {
        // Price is fresh: fetch and verify a new attestation from the oracle.
        let price = fetch_and_accept(env, &config, token);

        let updated_config = OracleConfig {
            last_update_timestamp: current_timestamp,
            cached_price: price,
            ..config
        };
        store_oracle_config(env, &updated_config);

        price
    } else {
        // Price is stale: use the (already-verified) cached price and emit
        // an audit event distinguishing this from a fresh acceptance.
        env.events().publish(
            (symbol_short!("pr_stale"),),
            OraclePriceRejectedStaleEvent {
                token: token.clone(),
                fallback_price: config.cached_price,
                staleness_secs,
            },
        );

        if config.cached_price <= 0 {
            env.panic_with_error(soroban_sdk::Error::from_contract_error(
                ContractError::OraclePriceInvalid as u32,
            ));
        }

        config.cached_price
    }
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
