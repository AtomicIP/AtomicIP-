//! Multi-Currency Payment Support Module
//!
//! Adds support for multiple payment currencies (XLM, USDC, EURC) in the
//! atomic swap contract.
//!
//! ## Fee-Asset Policy (#835)
//!
//! Protocol fees are **always collected in the configured `fee_asset` token**,
//! regardless of which currency a swap settles in. This prevents fee-accounting
//! drift when different counterparties settle in XLM, USDC, or EURC.
//!
//! ### Rules
//! 1. `MultiCurrencyConfig::fee_asset` is set once at initialisation and cannot
//!    be changed without an admin migration — it is the **single source of truth**
//!    for fee collection.
//! 2. Before a swap is finalised, `validate_fee_asset` MUST be called with the
//!    settlement token.  If the settlement token differs from `fee_asset`, the
//!    swap layer is expected to convert or reject — the swap MUST NOT collect
//!    fees in the settlement token directly.
//! 3. `collect_fee` returns the canonical fee amount denominated in `fee_asset`
//!    decimals so the caller can debit the correct amount from the right account.

use soroban_sdk::{contracttype, Address, Env, String, Vec};

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
/// `fee_asset` is the **only** token in which protocol fees are collected.
/// All other fields control which tokens may be used for swap settlement.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct MultiCurrencyConfig {
    pub enabled_tokens: Vec<SupportedToken>,
    pub default_token: SupportedToken,
    pub token_metadata: Vec<TokenMetadata>,
    /// The single canonical asset used for protocol fee collection.
    /// Defaults to `SupportedToken::XLM` and must not be changed without
    /// an authorised admin migration.
    pub fee_asset: SupportedToken,
}

/// Outcome returned by `validate_fee_asset`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum FeeAssetValidation {
    /// Settlement token matches the configured fee asset — fees may be
    /// collected directly in the settlement currency.
    Consistent,
    /// Settlement token differs from the configured fee asset — the caller
    /// MUST convert or reject; it must NOT collect fees in the settlement
    /// token.
    Inconsistent,
}

/// Result of a fee collection calculation.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FeeCalculation {
    /// Amount to collect, expressed in `fee_asset` decimals.
    pub fee_amount: i128,
    /// The token that must receive the fee.
    pub fee_asset: SupportedToken,
}

impl MultiCurrencyConfig {
    /// Build the default configuration (XLM, USDC, EURC enabled; XLM is fee asset).
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
            // XLM is the canonical fee asset by default.
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

    // ── Fee-asset policy (#835) ────────────────────────────────────────────────

    /// Check whether `settlement_token` is consistent with the configured
    /// `fee_asset`.
    ///
    /// Returns [`FeeAssetValidation::Consistent`] when they match, or
    /// [`FeeAssetValidation::Inconsistent`] when they differ.  Callers MUST
    /// act on an `Inconsistent` result — never collect fees directly in the
    /// settlement currency.
    pub fn validate_fee_asset(&self, settlement_token: &SupportedToken) -> FeeAssetValidation {
        if settlement_token == &self.fee_asset {
            FeeAssetValidation::Consistent
        } else {
            FeeAssetValidation::Inconsistent
        }
    }

    /// Calculate the protocol fee for a swap of `amount` settled in
    /// `settlement_token`, always denominating the result in `fee_asset`.
    ///
    /// `bps` is the fee rate in basis-points (e.g. 30 = 0.30 %).
    ///
    /// When `settlement_token` differs from `fee_asset` the `fee_amount` is
    /// still expressed in `fee_asset` units — the caller is responsible for
    /// any cross-asset conversion.  This keeps fee accounting in a single
    /// asset regardless of settlement currency.
    pub fn collect_fee(
        &self,
        amount: i128,
        bps: u32,
        settlement_token: &SupportedToken,
    ) -> FeeCalculation {
        // Fee is always expressed in the canonical fee_asset, not settlement_token.
        // When they differ the caller must handle the conversion; this function
        // simply enforces that fee denomination is always consistent.
        let _ = settlement_token; // policy: fee_asset wins, settlement_token ignored
        let fee_amount = amount * (bps as i128) / 10_000;
        FeeCalculation {
            fee_amount,
            fee_asset: self.fee_asset.clone(),
        }
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
    fn test_initialize_fee_asset_is_xlm() {
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env);
        assert_eq!(config.fee_asset, SupportedToken::XLM);
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

    // ── Fee-asset consistency tests (#835) ────────────────────────────────────

    /// XLM swap: settlement == fee_asset → Consistent
    #[test]
    fn test_fee_asset_validation_xlm_settlement_is_consistent() {
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env); // fee_asset = XLM
        let result = config.validate_fee_asset(&SupportedToken::XLM);
        assert_eq!(result, FeeAssetValidation::Consistent);
    }

    /// USDC swap: settlement ≠ fee_asset → Inconsistent; fees must NOT be in USDC
    #[test]
    fn test_fee_asset_validation_usdc_settlement_is_inconsistent() {
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env); // fee_asset = XLM
        let result = config.validate_fee_asset(&SupportedToken::USDC);
        assert_eq!(result, FeeAssetValidation::Inconsistent);
    }

    /// EURC swap: settlement ≠ fee_asset → Inconsistent; fees must NOT be in EURC
    #[test]
    fn test_fee_asset_validation_eurc_settlement_is_inconsistent() {
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env); // fee_asset = XLM
        let result = config.validate_fee_asset(&SupportedToken::EURC);
        assert_eq!(result, FeeAssetValidation::Inconsistent);
    }

    /// Custom token: settlement ≠ fee_asset → Inconsistent
    #[test]
    fn test_fee_asset_validation_custom_settlement_is_inconsistent() {
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env);
        let result = config.validate_fee_asset(&SupportedToken::Custom);
        assert_eq!(result, FeeAssetValidation::Inconsistent);
    }

    /// If fee_asset is explicitly set to USDC and settlement is USDC → Consistent
    #[test]
    fn test_fee_asset_validation_usdc_fee_asset_usdc_settlement_consistent() {
        let env = Env::default();
        let mut config = MultiCurrencyConfig::initialize(&env);
        config.fee_asset = SupportedToken::USDC; // override for this test
        let result = config.validate_fee_asset(&SupportedToken::USDC);
        assert_eq!(result, FeeAssetValidation::Consistent);
    }

    /// Fee is always denominated in fee_asset (XLM), regardless of settlement token.
    #[test]
    fn test_collect_fee_xlm_settlement_always_in_fee_asset() {
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env); // fee_asset = XLM
        let calc = config.collect_fee(10_000_000, 30, &SupportedToken::XLM);
        assert_eq!(calc.fee_asset, SupportedToken::XLM);
        assert_eq!(calc.fee_amount, 30_000); // 0.30 % of 10_000_000
    }

    /// Even when settling in USDC the fee is denominated in XLM (fee_asset).
    #[test]
    fn test_collect_fee_usdc_settlement_fee_still_in_xlm() {
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env); // fee_asset = XLM
        let calc = config.collect_fee(5_000_000, 30, &SupportedToken::USDC);
        assert_eq!(calc.fee_asset, SupportedToken::XLM); // fee always in XLM
        assert_eq!(calc.fee_amount, 15_000); // 0.30 % of 5_000_000
    }

    /// Even when settling in EURC the fee is denominated in XLM (fee_asset).
    #[test]
    fn test_collect_fee_eurc_settlement_fee_still_in_xlm() {
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env); // fee_asset = XLM
        let calc = config.collect_fee(2_000_000, 50, &SupportedToken::EURC);
        assert_eq!(calc.fee_asset, SupportedToken::XLM); // fee always in XLM
        assert_eq!(calc.fee_amount, 10_000); // 0.50 % of 2_000_000
    }

    /// Fee of zero amount is zero regardless of currency.
    #[test]
    fn test_collect_fee_zero_amount_all_currencies() {
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env);
        for token in [SupportedToken::XLM, SupportedToken::USDC, SupportedToken::EURC] {
            let calc = config.collect_fee(0, 30, &token);
            assert_eq!(calc.fee_amount, 0);
            assert_eq!(calc.fee_asset, SupportedToken::XLM);
        }
    }

    /// Full policy check: any swap not in fee_asset must be flagged Inconsistent,
    /// and the fee object must always name fee_asset.
    #[test]
    fn test_fee_policy_three_currencies_all_consistent_fee_asset() {
        let env = Env::default();
        let config = MultiCurrencyConfig::initialize(&env); // fee_asset = XLM

        let currencies = [
            (SupportedToken::XLM,  FeeAssetValidation::Consistent),
            (SupportedToken::USDC, FeeAssetValidation::Inconsistent),
            (SupportedToken::EURC, FeeAssetValidation::Inconsistent),
        ];

        for (token, expected_validation) in currencies {
            let validation = config.validate_fee_asset(&token);
            assert_eq!(validation, expected_validation,
                "validate_fee_asset({:?}) should be {:?}", token, expected_validation);

            // Regardless of settlement currency, fee_asset must be XLM in the
            // returned FeeCalculation.
            let calc = config.collect_fee(1_000_000, 30, &token);
            assert_eq!(calc.fee_asset, SupportedToken::XLM,
                "fee_asset must always be XLM for settlement in {:?}", token);
        }
    }
}
