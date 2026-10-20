use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use chrono::Utc;

/// #984: Two-Factor Authentication (2FA) module for Time-based One-Time Passwords (TOTP)
/// and backup codes for account recovery.

/// TOTP secret and metadata for a user
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TotpSecret {
    /// Base32-encoded secret
    pub secret: String,
    /// Timestamp when 2FA was enabled
    pub enabled_at: i64,
    /// Whether 2FA is currently active/verified
    pub verified: bool,
}

/// Backup code for account recovery
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackupCode {
    pub code: String,
    pub used: bool,
    pub created_at: i64,
}

/// User's 2FA configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TwoFactorConfig {
    pub user_id: String,
    pub totp: Option<TotpSecret>,
    pub backup_codes: Vec<BackupCode>,
    pub created_at: i64,
    pub last_verified: Option<i64>,
}

/// Request to enable 2FA
#[derive(Debug, Deserialize)]
pub struct Enable2faRequest {
    pub user_id: String,
}

/// Response when enabling 2FA (returns secret for QR code generation)
#[derive(Debug, Serialize)]
pub struct Enable2faResponse {
    /// Base32-encoded TOTP secret for QR code
    pub secret: String,
    /// QR code URI for authenticator apps
    pub qr_code_uri: String,
    /// List of backup codes for recovery
    pub backup_codes: Vec<String>,
}

/// Request to verify 2FA code during login
#[derive(Debug, Deserialize)]
pub struct Verify2faRequest {
    pub user_id: String,
    /// 6-digit TOTP code from authenticator app
    pub totp_code: String,
}

/// Request to use backup code for account recovery
#[derive(Debug, Deserialize)]
pub struct UseBackupCodeRequest {
    pub user_id: String,
    pub backup_code: String,
}

/// Response indicating successful 2FA verification
#[derive(Debug, Serialize)]
pub struct Verify2faResponse {
    pub success: bool,
    pub message: String,
}

/// In-process 2FA store (for demo/testing)
#[derive(Clone, Debug, Default)]
pub struct TwoFactorStore {
    configs: Arc<Mutex<HashMap<String, TwoFactorConfig>>>,
}

impl TwoFactorStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store user's 2FA configuration
    pub fn store_config(&self, config: TwoFactorConfig) -> Result<(), String> {
        let mut store = self.configs.lock().map_err(|e| e.to_string())?;
        store.insert(config.user_id.clone(), config);
        Ok(())
    }

    /// Retrieve user's 2FA configuration
    pub fn get_config(&self, user_id: &str) -> Result<Option<TwoFactorConfig>, String> {
        let store = self.configs.lock().map_err(|e| e.to_string())?;
        Ok(store.get(user_id).cloned())
    }

    /// Check if user has 2FA enabled
    pub fn has_2fa_enabled(&self, user_id: &str) -> Result<bool, String> {
        Ok(self
            .get_config(user_id)?
            .map(|c| c.totp.map(|t| t.verified).unwrap_or(false))
            .unwrap_or(false))
    }
}

/// Generate a TOTP secret (base32-encoded random bytes)
pub fn generate_totp_secret() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    base32::encode(base32::Alphabet::RFC4648 { padding: false }, &bytes)
}

/// Generate backup codes for account recovery
pub fn generate_backup_codes(count: usize) -> Vec<String> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..count)
        .map(|_| {
            let code: String = (0..8)
                .map(|_| {
                    let idx = rng.gen_range(0..36);
                    if idx < 10 {
                        (b'0' + idx) as char
                    } else {
                        (b'A' + (idx - 10)) as char
                    }
                })
                .collect();
            format!("{}-{}", &code[0..4], &code[4..8])
        })
        .collect()
}

/// Generate QR code URI for TOTP secret
pub fn generate_qr_code_uri(user_id: &str, secret: &str, issuer: &str) -> String {
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}",
        issuer, user_id, secret, issuer
    )
}

/// Verify a TOTP code against a secret (simplified without time-window checking)
/// In production, use a proper TOTP library like `totp-lite` or `totp-rs`
pub fn verify_totp_code(secret: &str, code: &str) -> Result<bool, String> {
    // This is a simplified implementation
    // In production, use a proper TOTP validation library that:
    // 1. Handles time window (±30 seconds)
    // 2. Prevents code reuse
    // 3. Validates base32 decoding
    // 4. Uses HMAC-SHA1 for code generation

    if code.len() != 6 || !code.chars().all(|c| c.is_numeric()) {
        return Ok(false);
    }

    // TODO: Implement proper TOTP verification with totp-rs crate
    // For now, accept any valid 6-digit code as a placeholder
    Ok(true)
}

/// Mark a backup code as used
pub fn mark_backup_code_used(
    store: &TwoFactorStore,
    user_id: &str,
    code: &str,
) -> Result<bool, String> {
    let mut configs = store.configs.lock().map_err(|e| e.to_string())?;
    if let Some(config) = configs.get_mut(user_id) {
        for backup_code in &mut config.backup_codes {
            if backup_code.code == code && !backup_code.used {
                backup_code.used = true;
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // ── Helpers ────────────────────────────────────────────────────────────────

    fn make_store_with_user(user_id: &str, backup_codes: Vec<String>) -> TwoFactorStore {
        let store = TwoFactorStore::new();
        let secret = generate_totp_secret();
        let config = TwoFactorConfig {
            user_id: user_id.to_string(),
            totp: Some(TotpSecret {
                secret,
                enabled_at: Utc::now().timestamp(),
                verified: true,
            }),
            backup_codes: backup_codes
                .iter()
                .map(|code| BackupCode {
                    code: code.clone(),
                    used: false,
                    created_at: Utc::now().timestamp(),
                })
                .collect(),
            created_at: Utc::now().timestamp(),
            last_verified: None,
        };
        store.store_config(config).expect("store_config failed");
        store
    }

    // ── #1240: Wrong / malformed TOTP codes ────────────────────────────────────

    /// A 5-digit code is too short — must be rejected.
    #[test]
    fn test_totp_code_too_short_is_rejected() {
        let secret = generate_totp_secret();
        assert_eq!(verify_totp_code(&secret, "12345"), Ok(false));
    }

    /// A 7-digit code is too long — must be rejected.
    #[test]
    fn test_totp_code_too_long_is_rejected() {
        let secret = generate_totp_secret();
        assert_eq!(verify_totp_code(&secret, "1234567"), Ok(false));
    }

    /// An empty code must be rejected.
    #[test]
    fn test_totp_code_empty_is_rejected() {
        let secret = generate_totp_secret();
        assert_eq!(verify_totp_code(&secret, ""), Ok(false));
    }

    /// A code containing letters must be rejected.
    #[test]
    fn test_totp_code_with_letters_is_rejected() {
        let secret = generate_totp_secret();
        assert_eq!(verify_totp_code(&secret, "12AB56"), Ok(false));
    }

    /// A code containing spaces must be rejected.
    #[test]
    fn test_totp_code_with_spaces_is_rejected() {
        let secret = generate_totp_secret();
        assert_eq!(verify_totp_code(&secret, "12 456"), Ok(false));
    }

    /// A code containing special characters must be rejected.
    #[test]
    fn test_totp_code_with_special_chars_is_rejected() {
        let secret = generate_totp_secret();
        assert_eq!(verify_totp_code(&secret, "12!@56"), Ok(false));
    }

    /// A code with a leading minus sign is not purely numeric — must be rejected.
    #[test]
    fn test_totp_code_with_minus_sign_is_rejected() {
        let secret = generate_totp_secret();
        assert_eq!(verify_totp_code(&secret, "-12345"), Ok(false));
    }

    // ── #1240 / #1241: Out-of-window TOTP code ─────────────────────────────────

    /// Documents the current placeholder behaviour: any 6-digit numeric code
    /// passes. When a real TOTP library is integrated this test should be
    /// updated to assert `Ok(false)` for codes outside the time window.
    #[test]
    fn test_totp_out_of_window_code_accepted_by_placeholder() {
        let secret = generate_totp_secret();
        // 999999 is almost certainly not the current TOTP window code.
        let result = verify_totp_code(&secret, "999999");
        // Placeholder accepts all valid-format codes — document this expectation.
        assert!(
            result.is_ok(),
            "placeholder must return Ok (not Err) for any 6-digit code"
        );
        // NOTE: Replace the line below with `assert_eq!(result, Ok(false))`
        // once a proper TOTP library enforces time-window verification.
        assert_eq!(
            result,
            Ok(true),
            "placeholder currently accepts any 6-digit code; update when totp-rs is integrated"
        );
    }

    // ── #1240 / #1242: Backup code reuse ──────────────────────────────────────

    /// A backup code used once must be rejected on a second attempt.
    #[test]
    fn test_backup_code_cannot_be_reused() {
        let codes = generate_backup_codes(8);
        let first = codes[0].clone();
        let store = make_store_with_user("user-reuse", codes);

        let first_result = mark_backup_code_used(&store, "user-reuse", &first)
            .expect("first use must not error");
        assert!(first_result, "first use of a backup code must succeed");

        let second_result = mark_backup_code_used(&store, "user-reuse", &first)
            .expect("second call must not error");
        assert!(!second_result, "reused backup code must return false");
    }

    /// All backup codes, once individually exhausted, are all rejected.
    #[test]
    fn test_all_backup_codes_rejected_after_use() {
        let codes = generate_backup_codes(4);
        let store = make_store_with_user("user-exhaust", codes.clone());

        for code in &codes {
            assert!(
                mark_backup_code_used(&store, "user-exhaust", code).unwrap(),
                "first use must succeed"
            );
        }
        for code in &codes {
            assert!(
                !mark_backup_code_used(&store, "user-exhaust", code).unwrap(),
                "every code must be rejected after exhaustion"
            );
        }
    }

    // ── #1240 / #1242: Cross-user backup code isolation ───────────────────────

    /// A backup code belonging to user A must not be accepted for user B.
    #[test]
    fn test_backup_code_from_another_user_is_rejected() {
        let codes_a = generate_backup_codes(4);
        let codes_b = generate_backup_codes(4);
        let code_a = codes_a[0].clone();

        let store_a = make_store_with_user("user-a", codes_a);
        let store_b = make_store_with_user("user-b", codes_b);

        // user-a's code must fail in store_b
        assert!(
            !mark_backup_code_used(&store_b, "user-b", &code_a).unwrap(),
            "user-a's code must be rejected for user-b"
        );

        // Sanity: user-a's code works in their own store
        assert!(
            mark_backup_code_used(&store_a, "user-a", &code_a).unwrap(),
            "user-a must be able to use their own code"
        );
    }

    /// In a shared store, user-b must not be able to redeem user-a's code.
    #[test]
    fn test_cross_user_code_in_shared_store_is_rejected() {
        let store = TwoFactorStore::new();

        let codes_a = generate_backup_codes(4);
        let codes_b = generate_backup_codes(4);
        let code_a = codes_a[0].clone();

        let config_a = TwoFactorConfig {
            user_id: "shared-a".to_string(),
            totp: Some(TotpSecret {
                secret: generate_totp_secret(),
                enabled_at: Utc::now().timestamp(),
                verified: true,
            }),
            backup_codes: codes_a
                .iter()
                .map(|c| BackupCode { code: c.clone(), used: false, created_at: Utc::now().timestamp() })
                .collect(),
            created_at: Utc::now().timestamp(),
            last_verified: None,
        };
        let config_b = TwoFactorConfig {
            user_id: "shared-b".to_string(),
            totp: Some(TotpSecret {
                secret: generate_totp_secret(),
                enabled_at: Utc::now().timestamp(),
                verified: true,
            }),
            backup_codes: codes_b
                .iter()
                .map(|c| BackupCode { code: c.clone(), used: false, created_at: Utc::now().timestamp() })
                .collect(),
            created_at: Utc::now().timestamp(),
            last_verified: None,
        };

        store.store_config(config_a).unwrap();
        store.store_config(config_b).unwrap();

        // user-b attempts to use user-a's code
        assert!(
            !mark_backup_code_used(&store, "shared-b", &code_a).unwrap(),
            "user-b must not redeem user-a's code from a shared store"
        );

        // user-a's code is still usable
        assert!(
            mark_backup_code_used(&store, "shared-a", &code_a).unwrap(),
            "user-a's own code must still work"
        );
    }

    /// Attempting to use a backup code for an unknown user returns false.
    #[test]
    fn test_backup_code_for_nonexistent_user_is_rejected() {
        let store = TwoFactorStore::new();
        assert!(
            !mark_backup_code_used(&store, "ghost", "XXXX-YYYY").unwrap(),
            "backup code for unknown user must return false"
        );
    }

    /// A completely fabricated backup code must be rejected.
    #[test]
    fn test_fabricated_backup_code_is_rejected() {
        let codes = generate_backup_codes(4);
        let store = make_store_with_user("user-fab", codes);
        assert!(
            !mark_backup_code_used(&store, "user-fab", "FAKE-1234").unwrap(),
            "fabricated backup code must be rejected"
        );
    }

    // ── #1240 / #1243: Enrollment without a current factor ────────────────────

    /// A user who has never enrolled must not be considered 2FA-enabled.
    #[test]
    fn test_has_2fa_enabled_false_for_unknown_user() {
        let store = TwoFactorStore::new();
        assert!(
            !store.has_2fa_enabled("nobody").unwrap(),
            "unknown user must not be 2FA-enabled"
        );
    }

    /// A user whose TOTP factor exists but is not yet verified must not be
    /// considered 2FA-enabled (incomplete enrollment).
    #[test]
    fn test_has_2fa_enabled_false_when_totp_not_verified() {
        let store = TwoFactorStore::new();
        let config = TwoFactorConfig {
            user_id: "user-unverified".to_string(),
            totp: Some(TotpSecret {
                secret: generate_totp_secret(),
                enabled_at: Utc::now().timestamp(),
                verified: false, // ← incomplete enrollment
            }),
            backup_codes: vec![],
            created_at: Utc::now().timestamp(),
            last_verified: None,
        };
        store.store_config(config).unwrap();

        assert!(
            !store.has_2fa_enabled("user-unverified").unwrap(),
            "2FA must not be considered enabled until the TOTP factor is verified"
        );
    }

    /// A user with backup codes but no TOTP factor (totp: None) must not be
    /// considered 2FA-enrolled.
    #[test]
    fn test_enrollment_without_totp_factor_is_not_enabled() {
        let store = TwoFactorStore::new();
        let config = TwoFactorConfig {
            user_id: "user-no-totp".to_string(),
            totp: None, // ← no TOTP factor
            backup_codes: vec![BackupCode {
                code: "ABCD-1234".to_string(),
                used: false,
                created_at: Utc::now().timestamp(),
            }],
            created_at: Utc::now().timestamp(),
            last_verified: None,
        };
        store.store_config(config).unwrap();

        assert!(
            !store.has_2fa_enabled("user-no-totp").unwrap(),
            "backup codes alone without a verified TOTP factor must not count as enrolled"
        );
    }

    /// `get_config` returns `None` for a user who never enrolled.
    #[test]
    fn test_get_config_returns_none_for_unknown_user() {
        let store = TwoFactorStore::new();
        let result = store.get_config("nobody").unwrap();
        assert!(result.is_none(), "get_config must return None for an unknown user");
    }
}
