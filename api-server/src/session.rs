use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// #985: Session management with timeout and grace period

/// Configuration for session timeout behavior
#[derive(Clone, Debug)]
pub struct SessionConfig {
    /// Idle timeout in minutes (default: 30)
    pub idle_timeout_minutes: i64,
    /// Grace period for refresh in minutes (default: 5)
    pub grace_period_minutes: i64,
    /// Enable warning before timeout
    pub enable_warning: bool,
    /// Minutes before timeout to show warning (default: 5)
    pub warning_minutes: i64,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            idle_timeout_minutes: 30,
            grace_period_minutes: 5,
            enable_warning: true,
            warning_minutes: 5,
        }
    }
}

/// Session information for a user
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    pub user_id: String,
    pub token: String,
    pub created_at: i64,
    pub last_activity: i64,
    pub expires_at: i64,
    pub grace_period_expires_at: Option<i64>,
}

/// Session status for the client
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionStatus {
    /// Whether session is active
    pub active: bool,
    /// Whether session is in grace period
    pub in_grace_period: bool,
    /// Minutes until timeout
    pub minutes_until_timeout: i64,
    /// Whether warning should be shown
    pub show_warning: bool,
    /// Message for user
    pub message: Option<String>,
}

/// Request to extend session
#[derive(Debug, Deserialize)]
pub struct ExtendSessionRequest {
    pub token: String,
}

/// Response when extending session
#[derive(Debug, Serialize)]
pub struct ExtendSessionResponse {
    pub success: bool,
    pub new_expiry: i64,
    pub message: String,
}

/// In-process session store
#[derive(Clone, Debug, Default)]
pub struct SessionStore {
    sessions: Arc<Mutex<HashMap<String, SessionInfo>>>,
    config: Arc<SessionConfig>,
}

impl SessionStore {
    pub fn new(config: SessionConfig) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            config: Arc::new(config),
        }
    }

    /// Create a new session for a user
    pub fn create_session(&self, user_id: &str, token: &str) -> Result<SessionInfo, String> {
        let now = Utc::now().timestamp();
        let expires_at = now + (self.config.idle_timeout_minutes * 60);

        let session = SessionInfo {
            user_id: user_id.to_string(),
            token: token.to_string(),
            created_at: now,
            last_activity: now,
            expires_at,
            grace_period_expires_at: None,
        };

        let mut store = self.sessions.lock().map_err(|e| e.to_string())?;
        store.insert(token.to_string(), session.clone());

        Ok(session)
    }

    /// Get session information
    pub fn get_session(&self, token: &str) -> Result<Option<SessionInfo>, String> {
        let store = self.sessions.lock().map_err(|e| e.to_string())?;
        Ok(store.get(token).cloned())
    }

    /// Update last activity timestamp
    pub fn update_activity(&self, token: &str) -> Result<(), String> {
        let mut store = self.sessions.lock().map_err(|e| e.to_string())?;
        if let Some(session) = store.get_mut(token) {
            let now = Utc::now().timestamp();
            session.last_activity = now;
            session.expires_at = now + (self.config.idle_timeout_minutes * 60);
        }
        Ok(())
    }

    /// Extend session expiration
    pub fn extend_session(&self, token: &str) -> Result<i64, String> {
        let mut store = self.sessions.lock().map_err(|e| e.to_string())?;
        if let Some(session) = store.get_mut(token) {
            let now = Utc::now().timestamp();
            session.last_activity = now;
            session.expires_at = now + (self.config.idle_timeout_minutes * 60);
            session.grace_period_expires_at = None;
            Ok(session.expires_at)
        } else {
            Err("Session not found".to_string())
        }
    }

    /// Check session status
    pub fn check_session_status(&self, token: &str) -> Result<SessionStatus, String> {
        let store = self.sessions.lock().map_err(|e| e.to_string())?;

        match store.get(token) {
            Some(session) => {
                let now = Utc::now().timestamp();
                let minutes_until_timeout = (session.expires_at - now) / 60;

                // Check if expired
                if now > session.expires_at {
                    // Check if still in grace period
                    if let Some(grace_expires) = session.grace_period_expires_at {
                        if now <= grace_expires {
                            return Ok(SessionStatus {
                                active: false,
                                in_grace_period: true,
                                minutes_until_timeout: (grace_expires - now) / 60,
                                show_warning: false,
                                message: Some("Session expired. Use /auth/extend-session with backup codes to recover.".to_string()),
                            });
                        }
                    } else {
                        // Set grace period
                        let mut store = self.sessions.lock().map_err(|e| e.to_string())?;
                        if let Some(session_mut) = store.get_mut(token) {
                            let grace_expires = now + (self.config.grace_period_minutes * 60);
                            session_mut.grace_period_expires_at = Some(grace_expires);
                            return Ok(SessionStatus {
                                active: false,
                                in_grace_period: true,
                                minutes_until_timeout: self.config.grace_period_minutes,
                                show_warning: false,
                                message: Some("Session expired. You have a grace period to extend your session.".to_string()),
                            });
                        }
                    }

                    // Grace period also expired
                    return Ok(SessionStatus {
                        active: false,
                        in_grace_period: false,
                        minutes_until_timeout: 0,
                        show_warning: false,
                        message: Some("Session expired and grace period ended. Please log in again.".to_string()),
                    });
                }

                // Check if warning should be shown
                let warning_threshold = self.config.warning_minutes * 60;
                let show_warning =
                    self.config.enable_warning && minutes_until_timeout <= self.config.warning_minutes;

                Ok(SessionStatus {
                    active: true,
                    in_grace_period: false,
                    minutes_until_timeout,
                    show_warning,
                    message: if show_warning {
                        Some(format!(
                            "Your session will expire in {} minutes.",
                            minutes_until_timeout
                        ))
                    } else {
                        None
                    },
                })
            }
            None => Err("Session not found".to_string()),
        }
    }

    /// Invalidate a session
    pub fn invalidate_session(&self, token: &str) -> Result<(), String> {
        let mut store = self.sessions.lock().map_err(|e| e.to_string())?;
        store.remove(token);
        Ok(())
    }

    /// Clean up expired sessions
    pub fn cleanup_expired_sessions(&self) -> Result<usize, String> {
        let mut store = self.sessions.lock().map_err(|e| e.to_string())?;
        let now = Utc::now().timestamp();
        let before = store.len();

        store.retain(|_, session| {
            if let Some(grace_expires) = session.grace_period_expires_at {
                now <= grace_expires
            } else {
                now <= session.expires_at
            }
        });

        Ok(before - store.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_get_session() {
        let store = SessionStore::new(SessionConfig::default());
        let session = store
            .create_session("user123", "token123")
            .expect("Failed to create session");

        assert_eq!(session.user_id, "user123");
        assert_eq!(session.token, "token123");

        let retrieved = store
            .get_session("token123")
            .expect("Failed to get session")
            .expect("Session not found");
        assert_eq!(retrieved.user_id, "user123");
    }

    #[test]
    fn test_session_timeout() {
        let mut config = SessionConfig::default();
        config.idle_timeout_minutes = 0; // Immediate timeout for testing
        let store = SessionStore::new(config);

        let session = store
            .create_session("user123", "token123")
            .expect("Failed to create session");

        std::thread::sleep(std::time::Duration::from_secs(1));

        let status = store
            .check_session_status("token123")
            .expect("Failed to check status");
        assert!(!status.active);
    }

    #[test]
    fn test_extend_session() {
        let store = SessionStore::new(SessionConfig::default());
        let session = store
            .create_session("user123", "token123")
            .expect("Failed to create session");

        let original_expiry = session.expires_at;

        std::thread::sleep(std::time::Duration::from_secs(1));

        let new_expiry = store
            .extend_session("token123")
            .expect("Failed to extend session");

        assert!(new_expiry > original_expiry);
    }
}
