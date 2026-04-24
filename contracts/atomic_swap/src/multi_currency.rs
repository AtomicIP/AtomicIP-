//! Multi-Currency Payment Support Module
//!
//! Adds support for multiple payment currencies (XLM, USDC, EURC) in the
//! atomic swap contract.

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
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct MultiCurrencyConfig {
    pub enabled_tokens: Vec<SupportedToken>,
    pub default_token: SupportedToken,
    pub token_metadata: Vec<TokenMetadata>,
}

impl MultiCurrencyConfig {
    /// Build the default configuration (XLM, USDC, EURC enabled).
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
}
