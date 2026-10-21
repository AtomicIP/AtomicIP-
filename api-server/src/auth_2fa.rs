use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use hmac::{Hmac, Mac};
use sha1::Sha1;

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
    /// Last successfully used TOTP time step, preventing code replay.
    #[serde(default)]
    pub last_totp_step: Option<u64>,
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
    /// Opaque token for the session created after successful verification.
    pub session_token: String,
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

    /// Create a user's 2FA configuration without replacing an existing one.
    pub fn create_config(&self, config: TwoFactorConfig) -> Result<bool, String> {
        let mut store = self.configs.lock().map_err(|e| e.to_string())?;
        if store.contains_key(&config.user_id) {
            return Ok(false);
        }
        store.insert(config.user_id.clone(), config);
        Ok(true)
    }

    /// Retrieve user's 2FA configuration
    pub fn get_config(&self, user_id: &str) -> Result<Option<TwoFactorConfig>, String> {
        let store = self.configs.lock().map_err(|e| e.to_string())?;
        Ok(store.get(user_id).cloned())
    }

    /// Remove user's 2FA configuration (#1107). Returns whether one existed.
    pub fn remove_config(&self, user_id: &str) -> Result<bool, String> {
        let mut store = self.configs.lock().map_err(|e| e.to_string())?;
        Ok(store.remove(user_id).is_some())
    }

    /// Check if user has 2FA enabled
    pub fn has_2fa_enabled(&self, user_id: &str) -> Result<bool, String> {
        Ok(self
            .get_config(user_id)?
            .map(|c| c.totp.map(|t| t.verified).unwrap_or(false))
            .unwrap_or(false))
    }

    /// Verify a code and record its time step under the same lock.
    pub fn verify_totp_code(
        &self,
        user_id: &str,
        code: &str,
        timestamp: i64,
    ) -> Result<Option<bool>, String> {
        let mut configs = self.configs.lock().map_err(|e| e.to_string())?;
        let Some(config) = configs.get_mut(user_id) else {
            return Ok(None);
        };
        let Some(totp) = config.totp.as_mut() else {
            return Ok(None);
        };
        let Some(step) = matching_totp_step(&totp.secret, code, timestamp)? else {
            return Ok(Some(false));
        };
        if totp.last_totp_step.is_some_and(|last| step <= last) {
            return Ok(Some(false));
        }

        totp.last_totp_step = Some(step);
        totp.verified = true;
        config.last_verified = Some(timestamp);
        Ok(Some(true))
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

/// Find the matching TOTP counter in the current 30-second window or either adjacent step.
pub fn matching_totp_step(
    secret: &str,
    code: &str,
    timestamp: i64,
) -> Result<Option<u64>, String> {
    if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok(None);
    }

    let key = base32::decode(base32::Alphabet::RFC4648 { padding: false }, secret)
        .ok_or_else(|| "Invalid base32 TOTP secret".to_string())?;
    if key.is_empty() {
        return Err("TOTP secret must not be empty".to_string());
    }

    let current_step = timestamp.max(0) as u64 / 30;
    for step in [
        current_step.saturating_sub(1),
        current_step,
        current_step.saturating_add(1),
    ] {
        let expected = generate_totp_code(&key, step)?;
        if constant_time_eq(expected.as_bytes(), code.as_bytes()) {
            return Ok(Some(step));
        }
    }
    Ok(None)
}

fn generate_totp_code(key: &[u8], step: u64) -> Result<String, String> {
    let mut mac = Hmac::<Sha1>::new_from_slice(key).map_err(|err| err.to_string())?;
    mac.update(&step.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = (digest[19] & 0x0f) as usize;
    let binary = (u32::from(digest[offset] & 0x7f) << 24)
        | (u32::from(digest[offset + 1]) << 16)
        | (u32::from(digest[offset + 2]) << 8)
        | u32::from(digest[offset + 3]);
    Ok(format!("{:06}", binary % 1_000_000))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
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

    fn config(user_id: &str, secret: &str) -> TwoFactorConfig {
        TwoFactorConfig {
            user_id: user_id.to_string(),
            totp: Some(TotpSecret {
                secret: secret.to_string(),
                enabled_at: 0,
                verified: true,
                last_totp_step: None,
            }),
            backup_codes: Vec::new(),
            created_at: 0,
            last_verified: None,
        }
    }

    #[test]
    fn create_config_does_not_replace_existing_secret() {
        let store = TwoFactorStore::new();
        assert!(store.create_config(config("user", "original")).unwrap());
        assert!(!store.create_config(config("user", "replacement")).unwrap());

        assert_eq!(
            store.get_config("user").unwrap().unwrap().totp.unwrap().secret,
            "original"
        );
    }

    #[test]
    fn matches_rfc_6238_sha1_test_vector() {
        assert_eq!(
            matching_totp_step(
                "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ",
                "287082",
                59
            )
            .unwrap(),
            Some(1)
        );
    }

    #[test]
    fn accepts_adjacent_totp_steps_only() {
        let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        let key = base32::decode(base32::Alphabet::RFC4648 { padding: false }, secret).unwrap();
        let adjacent_code = generate_totp_code(&key, 2).unwrap();
        let outside_window_code = generate_totp_code(&key, 3).unwrap();

        assert_eq!(
            matching_totp_step(secret, &adjacent_code, 30).unwrap(),
            Some(2)
        );
        assert_eq!(
            matching_totp_step(secret, &outside_window_code, 30).unwrap(),
            None
        );
    }

    #[test]
    fn rejects_invalid_totp_codes_and_secrets() {
        assert_eq!(
            matching_totp_step("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ", "123456", 59).unwrap(),
            None
        );
        assert_eq!(
            matching_totp_step("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ", "１２３４５６", 59).unwrap(),
            None
        );
        assert!(matching_totp_step("not-base32!", "287082", 59).is_err());
    }

    #[test]
    fn rejects_reuse_of_a_totp_step() {
        let store = TwoFactorStore::new();
        store
            .create_config(config("user", "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"))
            .unwrap();

        assert_eq!(
            store.verify_totp_code("user", "287082", 59).unwrap(),
            Some(true)
        );
        assert_eq!(
            store.verify_totp_code("user", "287082", 59).unwrap(),
            Some(false)
        );
    }
}
