//! Multi-Currency Payment Support Module
//!
//! Adds support for multiple payment currencies (XLM, USDC, EURC) in the
//! atomic swap contract.
//!
//! # Fee-Asset Policy
//!
//! ## Rationale
//!
//! When swaps can settle in different currencies the protocol fee must be
//! collected in a **single, pre-configured asset** — the *fee asset* — rather
//! than whichever token the swap happens to use.  Without this rule the
//! treasury would accumulate a mixture of tokens, making fee accounting,
//! auditing, and liquidation unnecessarily complex.
//!
//! ## Policy (canonical)
//!
//! 1. **One fee asset per deployment.**  The fee asset is chosen at contract
//!    initialisation time (or via an admin call) and stored under
//!    `DataKey::MultiCurrencyConfig`.  It defaults to `SupportedToken::XLM`.
//!
//! 2. **Fee is deducted in the fee asset, not the swap asset.**  When a swap
//!    settles in USDC the protocol fee is still collected in XLM (or whatever
//!    the fee asset is).  The swap contract is responsible for the conversion
//!    — either by holding a pre-funded fee reserve or by integrating with the
//!    on-chain price oracle.
//!
//!    *For the current implementation the fee is deducted from the settlement
//!    amount in the swap token and the result is converted to the fee asset
//!    at the oracle price.  If the fee asset is the same as the swap token,
//!    no conversion is needed.*
//!
//! 3. **Wrong-fee-asset payloads are rejected.**  If a caller attempts to pay
//!    a fee using a token that is not the configured fee asset the call panics
//!    with [`ContractError::InvalidFeeAsset`].  This prevents accidental or
//!    malicious fee-accounting drift.
//!
//! 4. **Supported swap tokens ≠ fee asset.**  A swap may settle in USDC while
//!    the fee is collected in XLM.  The `is_token_supported` check only gates
//!    *settlement* tokens; the `validate_fee_asset` check gates *fee* tokens.
//!
//! ## Summary table
//!
//! | Scenario | Swap token | Fee asset | Allowed? |
//! |---|---|---|---|
//! | XLM swap, fee in XLM | XLM | XLM | ✅ |
//! | USDC swap, fee in XLM | USDC | XLM | ✅ (conversion applied) |
//! | EURC swap, fee in XLM | EURC | XLM | ✅ (conversion applied) |
//! | USDC swap, fee in USDC | USDC | USDC | ❌ (rejected if fee asset ≠ USDC) |
//! | Custom token swap, fee in XLM | Custom | XLM | ✅ if custom token is supported |
//! | Any swap, fee in unsupported token | * | unsupported | ❌ |

use soroban_sdk::{contracttype, panic_with_error, Address, Env, String, Vec};

// Import the canonical error enum from the contract root (lib.rs).
// This avoids duplicating error codes and keeps the fee-asset error ordinal
// stable across upgrades.
use crate::ContractError;

/// Supported payment tokens.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum SupportedToken {
    XLM,    // Native XLM
    USDC,   // USD Coin
    EURC,   // Euro Coin
    Custom, // Custom token address
}

/// Token metadata for display and validation.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TokenMetadata {
    pub symbol: String,
    pub decimals: u32,
    /// `None` for native XLM; `Some(addr)` for SEP-41 tokens.
    pub address: Option<Address>,
    pub is_native: bool,
}

/// Multi-currency configuration stored on-chain.
///
/// The `fee_asset` field enforces the fee-asset policy described in the
/// module-level doc: all protocol fees are collected in this token regardless
/// of which token a swap settles in.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct MultiCurrencyConfig {
    pub enabled_tokens: Vec<SupportedToken>,
    pub default_token: SupportedToken,
    pub token_metadata: Vec<TokenMetadata>,
    /// The single asset in which protocol fees are always collected.
    /// Defaults to `SupportedToken::XLM`.  Must be in `enabled_tokens`.
    pub fee_asset: SupportedToken,
}

impl MultiCurrencyConfig {
    /// Build the default configuration (XLM, USDC, EURC enabled; fee asset = XLM).
    pub fn initialize(env: &Env) -> Self {
        let mut enabled_tokens = Vec::new(env);
        enabled_tokens.push_back(SupportedToken::XLM);
        enabled_tokens.push_back(SupportedToken::USDC);
        enabled_tokens.push_back(SupportedToken::EURC);

        let mut token_metadata = Vec::new(env);

        token_metadata.push_back(TokenMetadata {
            symbol: String::from_str(env, "XLM"),
            decimals: 7,
            address: None,
            is_native: true,
        });
        token_metadata.push_back(TokenMetadata {
            symbol: String::from_str(env, "USDC"),
            decimals: 6,
            address: None,
            is_native: false,
        });
        token_metadata.push_back(TokenMetadata {
            symbol: String::from_str(env, "EURC"),
            decimals: 6,
            address: None,
            is_native: false,
        });

        MultiCurrencyConfig {
            enabled_tokens,
            default_token: SupportedToken::XLM,
            token_metadata,
            // Policy default: fees are always collected in XLM.
            fee_asset: SupportedToken::XLM,
        }
    }

    /// Return `true` if `token` is in the enabled list.
    pub fn is_token_supported(&self, token: &SupportedToken) -> bool {
        self.enabled_tokens.contains(token.clone())
    }

    /// Find metadata by symbol (soroban `String` comparison).
    pub fn get_token_by_symbol(&self, _env: &Env, symbol: &String) -> Option<TokenMetadata> {
        for i in 0..self.token_metadata.len() {
            let meta = self.token_metadata.get(i).unwrap();
            if &meta.symbol == symbol {
                return Some(meta);
            }
        }
        None
    }

    /// Validate that `fee_token` matches the configured fee asset.
    ///
    /// Panics with [`ContractError::InvalidFeeAsset`] if the token does not
    /// match, enforcing the fee-asset consistency policy.
    ///
    /// Call this at the point where the protocol fee is about to be deducted
    /// so that wrong-fee-asset payloads are rejected early.
    pub fn validate_fee_asset(&self, env: &Env, fee_token: &SupportedToken) {
        if fee_token != &self.fee_asset {
            panic_with_error!(env, ContractError::InvalidFeeAsset);
        }
    }

    /// Return `true` if `fee_token` is the configured fee asset.
    ///
    /// Prefer [`validate_fee_asset`] for contract calls that must reject
    /// mis-matched tokens.  Use this predicate in tests and off-chain logic
    /// that needs a boolean rather than a panic.
    pub fn is_valid_fee_asset(&self, fee_token: &SupportedToken) -> bool {
        fee_token == &self.fee_asset
    }
}

// ── Events ────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TokenAddedEvent {
    pub token: SupportedToken,
    pub address: Option<Address>,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TokenRemovedEvent {
    pub token: SupportedToken,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn test_supported_token_variants_are_distinct() {
        assert_ne!(SupportedToken::XLM, SupportedToken::USDC);
        assert_ne!(SupportedToken::USDC, SupportedToken::EURC);
        assert_ne!(SupportedToken::XLM, SupportedToken::EURC);
    }

    #[test]
    fn test_initialize_enables_three_tokens() {
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env);
        assert!(config.is_token_supported(&SupportedToken::XLM));
        assert!(config.is_token_supported(&SupportedToken::USDC));
        assert!(config.is_token_supported(&SupportedToken::EURC));
        assert!(!config.is_token_supported(&SupportedToken::Custom));
    }

    #[test]
    fn test_get_token_by_symbol_found() {
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env);
        let sym = String::from_str(&env, "USDC");
        let meta = config.get_token_by_symbol(&env, &sym);
        assert!(meta.is_some());
        assert_eq!(meta.unwrap().decimals, 6);
    }

    #[test]
    fn test_get_token_by_symbol_not_found() {
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env);
        let sym = String::from_str(&env, "BTC");
        assert!(config.get_token_by_symbol(&env, &sym).is_none());
    }

    // ── #835: Fee-asset policy tests ──────────────────────────────────────────

    #[test]
    fn test_default_fee_asset_is_xlm() {
        // Policy: fees default to XLM regardless of which token a swap settles in.
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env);
        assert_eq!(
            config.fee_asset,
            SupportedToken::XLM,
            "default fee asset must be XLM"
        );
    }

    #[test]
    fn test_fee_asset_xlm_is_valid_fee_asset() {
        // XLM swap pays fee in XLM — matches the default fee asset.
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env);
        assert!(
            config.is_valid_fee_asset(&SupportedToken::XLM),
            "XLM must be a valid fee asset when fee_asset=XLM"
        );
    }

    #[test]
    fn test_fee_asset_usdc_is_invalid_when_fee_asset_is_xlm() {
        // USDC swap must NOT pay fee in USDC when the configured fee asset is XLM.
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env);
        assert!(
            !config.is_valid_fee_asset(&SupportedToken::USDC),
            "USDC must be rejected as fee asset when fee_asset=XLM"
        );
    }

    #[test]
    fn test_fee_asset_eurc_is_invalid_when_fee_asset_is_xlm() {
        // EURC swap must NOT pay fee in EURC when the configured fee asset is XLM.
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env);
        assert!(
            !config.is_valid_fee_asset(&SupportedToken::EURC),
            "EURC must be rejected as fee asset when fee_asset=XLM"
        );
    }

    #[test]
    #[should_panic]
    fn test_validate_fee_asset_panics_for_usdc_when_fee_asset_is_xlm() {
        // Calling validate_fee_asset with USDC when the fee asset is XLM
        // must panic — this is the guard that prevents fee-accounting drift.
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env);
        config.validate_fee_asset(&env, &SupportedToken::USDC);
    }

    #[test]
    #[should_panic]
    fn test_validate_fee_asset_panics_for_eurc_when_fee_asset_is_xlm() {
        // Same guard for EURC.
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env);
        config.validate_fee_asset(&env, &SupportedToken::EURC);
    }

    #[test]
    fn test_validate_fee_asset_succeeds_for_xlm_when_fee_asset_is_xlm() {
        // Must not panic when the correct fee asset is supplied.
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env);
        // Should not panic
        config.validate_fee_asset(&env, &SupportedToken::XLM);
    }

    #[test]
    fn test_fee_asset_can_be_reconfigured_to_usdc() {
        // An admin can change the fee asset to USDC; subsequent validation
        // must accept USDC and reject XLM.
        let env = Env::default();
        let mut config = MultiCurrencyConfig::initialize(&env);
        config.fee_asset = SupportedToken::USDC;

        assert!(config.is_valid_fee_asset(&SupportedToken::USDC), "USDC must be valid after reconfigure");
        assert!(!config.is_valid_fee_asset(&SupportedToken::XLM), "XLM must be invalid after reconfigure to USDC");
        assert!(!config.is_valid_fee_asset(&SupportedToken::EURC), "EURC must be invalid after reconfigure to USDC");
    }

    #[test]
    fn test_fee_asset_can_be_reconfigured_to_eurc() {
        // Fee asset reconfigured to EURC.
        let env = Env::default();
        let mut config = MultiCurrencyConfig::initialize(&env);
        config.fee_asset = SupportedToken::EURC;

        assert!(config.is_valid_fee_asset(&SupportedToken::EURC), "EURC must be valid after reconfigure");
        assert!(!config.is_valid_fee_asset(&SupportedToken::XLM), "XLM must be invalid after reconfigure to EURC");
        assert!(!config.is_valid_fee_asset(&SupportedToken::USDC), "USDC must be invalid after reconfigure to EURC");
    }

    #[test]
    fn test_usdc_swap_with_xlm_fee_asset_is_supported_combination() {
        // USDC is a supported *swap* token even though the fee asset is XLM.
        // These are independent checks: is_token_supported gates settlement;
        // is_valid_fee_asset gates fee collection.
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env);

        // USDC is a valid settlement token
        assert!(config.is_token_supported(&SupportedToken::USDC));
        // XLM is the required fee token, not USDC
        assert!(!config.is_valid_fee_asset(&SupportedToken::USDC));
        assert!(config.is_valid_fee_asset(&SupportedToken::XLM));
    }

    #[test]
    fn test_eurc_swap_with_xlm_fee_asset_is_supported_combination() {
        // EURC is a supported *swap* token even though the fee asset is XLM.
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env);

        assert!(config.is_token_supported(&SupportedToken::EURC));
        assert!(!config.is_valid_fee_asset(&SupportedToken::EURC));
        assert!(config.is_valid_fee_asset(&SupportedToken::XLM));
    }

    #[test]
    fn test_fee_asset_must_be_in_enabled_tokens() {
        // The fee asset should always be in the enabled tokens list.
        // This verifies the invariant is maintained by initialize().
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env);
        assert!(
            config.is_token_supported(&config.fee_asset),
            "fee_asset must always be in enabled_tokens"
        );
    }
}
