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
