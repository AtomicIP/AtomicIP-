//! Soroban RPC client for invoking on-chain contract functions.
//!
//! This module provides:
//! * [`SorobanRpcError`] — canonical error type for all RPC failures.
//! * [`map_rpc_error_to_status`] — converts `SorobanRpcError` to the
//!   appropriate HTTP `StatusCode` for use in handler responses.
//! * [`SorobanRpcClient`] trait — the interface every handler calls.
//! * [`LiveSorobanRpcClient`] — production implementation using `reqwest` to
//!   call the Stellar Soroban JSON-RPC endpoint.
//! * [`MockSorobanRpcClient`] — in-process stub used by unit and integration
//!   tests (no network required).
//!
//! ## Soroban JSON-RPC basics
//!
//! Every contract invocation goes through two RPC calls:
//!
//! 1. `simulateTransaction` — dry-runs the XDR-encoded transaction and returns
//!    the footprint, auth entries, and resource fees.
//! 2. `sendTransaction` — broadcasts the signed transaction and returns the
//!    transaction hash.
//!
//! For the purpose of this handler layer we simulate the call and return the
//! result value; we rely on the caller's wallet layer for actual signing and
//! submission.  The `commit_ip` handler therefore calls `simulateTransaction`
//! to validate inputs and retrieve the would-be return value (the new IP ID),
//! then the result is returned to the API client which is expected to sign and
//! submit the final transaction independently.

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::env;

// ── Error type ────────────────────────────────────────────────────────────────

/// All errors that can arise from a Soroban RPC call.
#[derive(Debug, Clone, PartialEq)]
pub enum SorobanRpcError {
    /// The request was malformed (e.g., invalid XDR, bad address format).
    InvalidInput(String),
    /// The contract rejected the call (e.g., duplicate hash, not authorised).
    ContractError(String),
    /// The requested resource (IP ID, swap ID) does not exist on-chain.
    NotFound(String),
    /// The RPC node is unavailable or the call timed out.
    Unavailable(String),
    /// An unexpected internal error occurred.
    Internal(String),
}

impl std::fmt::Display for SorobanRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SorobanRpcError::InvalidInput(msg)   => write!(f, "invalid input: {}", msg),
            SorobanRpcError::ContractError(msg)  => write!(f, "contract error: {}", msg),
            SorobanRpcError::NotFound(msg)       => write!(f, "not found: {}", msg),
            SorobanRpcError::Unavailable(msg)    => write!(f, "rpc unavailable: {}", msg),
            SorobanRpcError::Internal(msg)       => write!(f, "internal error: {}", msg),
        }
    }
}

/// Map a [`SorobanRpcError`] to the HTTP [`StatusCode`] that best describes it.
///
/// | Error variant          | HTTP status |
/// |------------------------|-------------|
/// | `InvalidInput`         | 400         |
/// | `ContractError`        | 400         |
/// | `NotFound`             | 404         |
/// | `Unavailable`          | 503         |
/// | `Internal`             | 500         |
pub fn map_rpc_error_to_status(err: &SorobanRpcError) -> StatusCode {
    match err {
        SorobanRpcError::InvalidInput(_)  => StatusCode::BAD_REQUEST,
        SorobanRpcError::ContractError(_) => StatusCode::BAD_REQUEST,
        SorobanRpcError::NotFound(_)      => StatusCode::NOT_FOUND,
        SorobanRpcError::Unavailable(_)   => StatusCode::SERVICE_UNAVAILABLE,
        SorobanRpcError::Internal(_)      => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// ── Trait ─────────────────────────────────────────────────────────────────────

/// Interface for calling Soroban smart-contract functions from the API layer.
///
/// Keeping this behind a trait means handlers can be tested with
/// [`MockSorobanRpcClient`] and deployed with [`LiveSorobanRpcClient`] without
/// changing any handler code.
#[async_trait::async_trait]
pub trait SorobanRpcClient: Send + Sync {
    /// Invoke `ip_registry.commit_ip(owner, commitment_hash)` and return the
    /// newly assigned IP ID.
    async fn commit_ip(
        &self,
        owner: &str,
        commitment_hash: &str,
    ) -> Result<u64, SorobanRpcError>;
}

// ── JSON-RPC wire types ───────────────────────────────────────────────────────

/// Outbound JSON-RPC request envelope.
#[derive(Debug, Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'a str,
    id: u64,
    method: &'a str,
    params: serde_json::Value,
}

/// Inbound JSON-RPC response envelope.
#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    id: serde_json::Value,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

// ── Live implementation ───────────────────────────────────────────────────────

/// Production Soroban RPC client.
///
/// Reads configuration from environment variables:
/// * `SOROBAN_RPC_URL`      — JSON-RPC endpoint (default: `https://soroban-testnet.stellar.org`)
/// * `IP_REGISTRY_CONTRACT` — contract address of the deployed `ip_registry` contract
pub struct LiveSorobanRpcClient {
    http:            reqwest::Client,
    rpc_url:         String,
    contract_id:     String,
}

impl LiveSorobanRpcClient {
    /// Create a new client, reading `SOROBAN_RPC_URL` and
    /// `IP_REGISTRY_CONTRACT` from the environment.
    pub fn from_env() -> Self {
        let rpc_url = env::var("SOROBAN_RPC_URL")
            .unwrap_or_else(|_| "https://soroban-testnet.stellar.org".to_string());
        let contract_id = env::var("IP_REGISTRY_CONTRACT")
            .unwrap_or_else(|_| "".to_string());

        Self {
            http: reqwest::Client::new(),
            rpc_url,
            contract_id,
        }
    }

    /// Build a minimal `simulateTransaction` payload for `commit_ip`.
    ///
    /// A real implementation would XDR-encode a `StellarTransaction`; here we
    /// use a placeholder XDR value so the wire format is established without
    /// pulling in the full XDR crate.  The handler validates inputs before
    /// calling this, so callers receive a meaningful error for bad inputs even
    /// before the RPC hop.
    fn build_simulate_payload(&self, owner: &str, commitment_hash: &str) -> serde_json::Value {
        // In production this would be a base64-encoded XDR TransactionEnvelope.
        // We represent it as a structured object so integration tests can inspect
        // the payload without XDR decoding.
        serde_json::json!({
            "transaction": {
                "contract_id": self.contract_id,
                "function":    "commit_ip",
                "args": {
                    "owner":           owner,
                    "commitment_hash": commitment_hash
                }
            }
        })
    }

    /// Parse the `simulateTransaction` result and extract the returned `u64`
    /// IP ID from the result value.
    fn parse_commit_ip_result(result: &serde_json::Value) -> Result<u64, SorobanRpcError> {
        // The Soroban RPC returns the contract's return value under
        // `result.retval` as a base64-encoded XDR `ScVal`.  We handle the
        // common testnet JSON format where `retval` may be a plain integer for
        // simplicity in a sandboxed environment.
        let retval = result
            .get("retval")
            .or_else(|| result.get("ip_id"))
            .or_else(|| result.get("result"));

        match retval {
            Some(serde_json::Value::Number(n)) => {
                n.as_u64().ok_or_else(|| {
                    SorobanRpcError::Internal("ip_id returned by contract is not a valid u64".to_string())
                })
            }
            Some(serde_json::Value::String(s)) => {
                // Could be a base64-encoded ScVal — try to parse as a decimal
                // integer string first (sandboxed/testnet shorthand).
                s.parse::<u64>().map_err(|_| {
                    SorobanRpcError::Internal(format!(
                        "could not parse ip_id from contract result string: {}", s
                    ))
                })
            }
            _ => Err(SorobanRpcError::Internal(
                "unexpected return value format from ip_registry.commit_ip".to_string(),
            )),
        }
    }
}

#[async_trait::async_trait]
impl SorobanRpcClient for LiveSorobanRpcClient {
    async fn commit_ip(
        &self,
        owner: &str,
        commitment_hash: &str,
    ) -> Result<u64, SorobanRpcError> {
        // ── Input validation ──────────────────────────────────────────────────
        if owner.is_empty() {
            return Err(SorobanRpcError::InvalidInput(
                "owner address must not be empty".to_string(),
            ));
        }
        if commitment_hash.is_empty() {
            return Err(SorobanRpcError::InvalidInput(
                "commitment_hash must not be empty".to_string(),
            ));
        }
        // A Pedersen commitment hash is exactly 32 bytes → 64 hex characters.
        let hex_len = commitment_hash.len();
        if hex_len != 64 {
            return Err(SorobanRpcError::InvalidInput(format!(
                "commitment_hash must be 64 hex characters (32 bytes), got {}",
                hex_len
            )));
        }
        if !commitment_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(SorobanRpcError::InvalidInput(
                "commitment_hash must be hex-encoded".to_string(),
            ));
        }
        if self.contract_id.is_empty() {
            return Err(SorobanRpcError::Unavailable(
                "IP_REGISTRY_CONTRACT env var is not set; cannot invoke Soroban RPC".to_string(),
            ));
        }

        // ── RPC call ──────────────────────────────────────────────────────────
        let payload = self.build_simulate_payload(owner, commitment_hash);
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "simulateTransaction",
            params: serde_json::json!([payload]),
        };

        let response = self
            .http
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| SorobanRpcError::Unavailable(e.to_string()))?;

        let status = response.status();
        let body: JsonRpcResponse = response
            .json()
            .await
            .map_err(|e| SorobanRpcError::Internal(e.to_string()))?;

        // ── Error mapping ─────────────────────────────────────────────────────
        if !status.is_success() {
            return Err(SorobanRpcError::Unavailable(format!(
                "Soroban RPC returned HTTP {}",
                status
            )));
        }

        if let Some(err) = body.error {
            // JSON-RPC error codes:
            // -32602  invalid params / invalid input
            // 1xxx    contract-level errors
            let rpc_err = match err.code {
                -32602 | -32600 => {
                    SorobanRpcError::InvalidInput(err.message)
                }
                1001..=1999 => {
                    // Contract returned an error (e.g., duplicate commitment hash)
                    SorobanRpcError::ContractError(err.message)
                }
                _ => SorobanRpcError::Internal(format!(
                    "Soroban RPC error {}: {}",
                    err.code, err.message
                )),
            };
            return Err(rpc_err);
        }

        let result = body.result.ok_or_else(|| {
            SorobanRpcError::Internal(
                "Soroban RPC returned neither result nor error for commit_ip".to_string(),
            )
        })?;

        Self::parse_commit_ip_result(&result)
    }
}

// ── Mock implementation ───────────────────────────────────────────────────────

/// In-process mock for tests. Returns predictable values without any network.
///
/// By default every call succeeds and returns IP ID `1`.
/// Use [`MockSorobanRpcClient::with_error`] to test error paths.
#[derive(Clone, Default)]
pub struct MockSorobanRpcClient {
    /// When `Some`, every call returns this error instead of a success.
    pub force_error: Option<SorobanRpcError>,
    /// Simulated next IP ID returned by `commit_ip` (default: 1).
    pub next_ip_id: Option<u64>,
}

impl MockSorobanRpcClient {
    /// Create a mock that always returns the given error.
    pub fn with_error(err: SorobanRpcError) -> Self {
        MockSorobanRpcClient {
            force_error: Some(err),
            next_ip_id: None,
        }
    }

    /// Create a mock that returns the given IP ID on success.
    pub fn with_ip_id(id: u64) -> Self {
        MockSorobanRpcClient {
            force_error: None,
            next_ip_id: Some(id),
        }
    }
}

#[async_trait::async_trait]
impl SorobanRpcClient for MockSorobanRpcClient {
    async fn commit_ip(
        &self,
        owner: &str,
        commitment_hash: &str,
    ) -> Result<u64, SorobanRpcError> {
        if let Some(ref err) = self.force_error {
            return Err(err.clone());
        }
        // Basic validation mirrors the live client so tests exercise the same
        // input-validation logic even without a real RPC endpoint.
        if owner.is_empty() {
            return Err(SorobanRpcError::InvalidInput(
                "owner address must not be empty".to_string(),
            ));
        }
        if commitment_hash.len() != 64 {
            return Err(SorobanRpcError::InvalidInput(format!(
                "commitment_hash must be 64 hex characters, got {}",
                commitment_hash.len()
            )));
        }
        Ok(self.next_ip_id.unwrap_or(1))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── map_rpc_error_to_status ───────────────────────────────────────────────

    #[test]
    fn test_invalid_input_maps_to_400() {
        let err = SorobanRpcError::InvalidInput("bad param".to_string());
        assert_eq!(map_rpc_error_to_status(&err), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_contract_error_maps_to_400() {
        let err = SorobanRpcError::ContractError("duplicate hash".to_string());
        assert_eq!(map_rpc_error_to_status(&err), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_not_found_maps_to_404() {
        let err = SorobanRpcError::NotFound("ip 99 not found".to_string());
        assert_eq!(map_rpc_error_to_status(&err), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_unavailable_maps_to_503() {
        let err = SorobanRpcError::Unavailable("rpc timeout".to_string());
        assert_eq!(map_rpc_error_to_status(&err), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn test_internal_maps_to_500() {
        let err = SorobanRpcError::Internal("unexpected".to_string());
        assert_eq!(map_rpc_error_to_status(&err), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ── MockSorobanRpcClient::commit_ip ───────────────────────────────────────

    #[tokio::test]
    async fn test_mock_commit_ip_success_returns_default_id() {
        let mock = MockSorobanRpcClient::default();
        let result = mock
            .commit_ip(
                "GABC1234567890ABCDEF",
                "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
            )
            .await;
        assert_eq!(result.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_mock_commit_ip_custom_id() {
        let mock = MockSorobanRpcClient::with_ip_id(42);
        let result = mock
            .commit_ip(
                "GABC1234567890ABCDEF",
                "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
            )
            .await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_mock_commit_ip_empty_owner_rejected() {
        let mock = MockSorobanRpcClient::default();
        let result = mock
            .commit_ip(
                "",
                "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
            )
            .await;
        assert!(matches!(result, Err(SorobanRpcError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_mock_commit_ip_short_hash_rejected() {
        let mock = MockSorobanRpcClient::default();
        let result = mock.commit_ip("GABC123", "aabbcc").await;
        assert!(matches!(result, Err(SorobanRpcError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_mock_commit_ip_forced_contract_error() {
        let mock = MockSorobanRpcClient::with_error(SorobanRpcError::ContractError(
            "commitment hash already registered".to_string(),
        ));
        let result = mock
            .commit_ip(
                "GABC1234567890ABCDEF",
                "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
            )
            .await;
        assert!(matches!(result, Err(SorobanRpcError::ContractError(_))));
    }

    #[tokio::test]
    async fn test_mock_commit_ip_forced_unavailable() {
        let mock = MockSorobanRpcClient::with_error(SorobanRpcError::Unavailable(
            "rpc node down".to_string(),
        ));
        let result = mock
            .commit_ip(
                "GABC1234567890ABCDEF",
                "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
            )
            .await;
        assert!(matches!(result, Err(SorobanRpcError::Unavailable(_))));
        assert_eq!(
            map_rpc_error_to_status(result.as_ref().unwrap_err()),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    // ── LiveSorobanRpcClient input validation (no network) ────────────────────

    #[tokio::test]
    async fn test_live_client_empty_owner_returns_invalid_input() {
        let client = LiveSorobanRpcClient {
            http: reqwest::Client::new(),
            rpc_url: "http://localhost:8000".to_string(),
            contract_id: "CONTRACT123".to_string(),
        };
        let result = client
            .commit_ip(
                "",
                "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
            )
            .await;
        assert!(matches!(result, Err(SorobanRpcError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_live_client_short_hash_returns_invalid_input() {
        let client = LiveSorobanRpcClient {
            http: reqwest::Client::new(),
            rpc_url: "http://localhost:8000".to_string(),
            contract_id: "CONTRACT123".to_string(),
        };
        let result = client
            .commit_ip("GOWNER123", "tooshort")
            .await;
        assert!(matches!(result, Err(SorobanRpcError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_live_client_non_hex_hash_returns_invalid_input() {
        let client = LiveSorobanRpcClient {
            http: reqwest::Client::new(),
            rpc_url: "http://localhost:8000".to_string(),
            contract_id: "CONTRACT123".to_string(),
        };
        // 64 chars but with invalid hex characters (Z, spaces)
        let result = client
            .commit_ip(
                "GOWNER123",
                "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ",
            )
            .await;
        assert!(matches!(result, Err(SorobanRpcError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_live_client_missing_contract_id_returns_unavailable() {
        let client = LiveSorobanRpcClient {
            http: reqwest::Client::new(),
            rpc_url: "http://localhost:8000".to_string(),
            contract_id: "".to_string(), // not configured
        };
        let result = client
            .commit_ip(
                "GOWNER123",
                "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
            )
            .await;
        assert!(matches!(result, Err(SorobanRpcError::Unavailable(_))));
    }

    // ── parse_commit_ip_result ────────────────────────────────────────────────

    #[test]
    fn test_parse_commit_ip_result_from_number() {
        let val = serde_json::json!({ "retval": 7 });
        let ip_id = LiveSorobanRpcClient::parse_commit_ip_result(&val).unwrap();
        assert_eq!(ip_id, 7);
    }

    #[test]
    fn test_parse_commit_ip_result_from_string() {
        let val = serde_json::json!({ "retval": "42" });
        let ip_id = LiveSorobanRpcClient::parse_commit_ip_result(&val).unwrap();
        assert_eq!(ip_id, 42);
    }

    #[test]
    fn test_parse_commit_ip_result_fallback_ip_id_field() {
        let val = serde_json::json!({ "ip_id": 99 });
        let ip_id = LiveSorobanRpcClient::parse_commit_ip_result(&val).unwrap();
        assert_eq!(ip_id, 99);
    }

    #[test]
    fn test_parse_commit_ip_result_missing_field_is_error() {
        let val = serde_json::json!({ "something_else": "x" });
        let result = LiveSorobanRpcClient::parse_commit_ip_result(&val);
        assert!(matches!(result, Err(SorobanRpcError::Internal(_))));
    }
}
