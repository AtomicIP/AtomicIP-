//! Soroban RPC client module.
//!
//! Provides a thin async client that wraps the Stellar Soroban JSON-RPC API,
//! mapping contract invocation responses to strongly-typed Rust results.
//!
//! The RPC endpoint is read from the `SOROBAN_RPC_URL` environment variable
//! (default: `https://soroban-testnet.stellar.org`), and the contract IDs
//! are read from `IP_REGISTRY_CONTRACT_ID` and `ATOMIC_SWAP_CONTRACT_ID`.
//!
//! # Error mapping
//!
//! Soroban contract errors are forwarded as [`SorobanError`] variants so that
//! HTTP handlers can map them to the correct status codes without inspecting
//! raw JSON.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;

// ── Configuration ──────────────────────────────────────────────────────────────

fn rpc_url() -> String {
    env::var("SOROBAN_RPC_URL")
        .unwrap_or_else(|_| "https://soroban-testnet.stellar.org".to_string())
}

fn ip_registry_contract_id() -> String {
    env::var("IP_REGISTRY_CONTRACT_ID").unwrap_or_else(|_| "IP_REGISTRY_CONTRACT_ID_NOT_SET".to_string())
}

// ── JSON-RPC types ─────────────────────────────────────────────────────────────

/// A single JSON-RPC 2.0 request envelope.
#[derive(Debug, Serialize)]
struct RpcRequest<'a> {
    jsonrpc: &'a str,
    id: u64,
    method: &'a str,
    params: serde_json::Value,
}

/// A JSON-RPC 2.0 response envelope.
#[derive(Debug, Deserialize)]
struct RpcResponse {
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<RpcErrorObject>,
}

/// JSON-RPC error object.
#[derive(Debug, Deserialize)]
struct RpcErrorObject {
    code: i64,
    message: String,
    #[serde(default)]
    data: Option<serde_json::Value>,
}

// ── Soroban-specific error types ───────────────────────────────────────────────

/// Errors that can be returned by Soroban contract invocations.
#[derive(Debug, Clone, PartialEq)]
pub enum SorobanError {
    /// The commitment hash is all-zeroes (rejected by contract).
    ZeroHash,
    /// A commitment with this hash is already registered.
    DuplicateHash,
    /// The owner address is invalid.
    InvalidOwner,
    /// The IP record does not exist.
    NotFound,
    /// Caller is not the IP owner.
    NotOwner,
    /// The IP record has been revoked.
    Revoked,
    /// The contract is not initialized.
    NotInitialized,
    /// The RPC request itself failed (network error, timeout, etc.).
    RpcFailure(String),
    /// An unexpected error from the contract.
    ContractError(String),
}

impl std::fmt::Display for SorobanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SorobanError::ZeroHash => write!(f, "Commitment hash must not be all zeroes"),
            SorobanError::DuplicateHash => write!(f, "A commitment with this hash already exists"),
            SorobanError::InvalidOwner => write!(f, "Invalid owner address"),
            SorobanError::NotFound => write!(f, "IP record not found"),
            SorobanError::NotOwner => write!(f, "Caller is not the IP owner"),
            SorobanError::Revoked => write!(f, "IP record has been revoked"),
            SorobanError::NotInitialized => write!(f, "Contract is not initialized"),
            SorobanError::RpcFailure(msg) => write!(f, "RPC failure: {msg}"),
            SorobanError::ContractError(msg) => write!(f, "Contract error: {msg}"),
        }
    }
}

/// Map a raw Soroban JSON-RPC error message to a typed [`SorobanError`].
///
/// Soroban contract panics surface as `"Error(Contract, #N)"` strings in the
/// RPC error data, where `N` matches the `ContractError` enum ordinal defined
/// in each contract's `errors.rs`.  We recognise common patterns here; any
/// unrecognised message falls back to [`SorobanError::ContractError`].
fn classify_error(message: &str, data: Option<&serde_json::Value>) -> SorobanError {
    // Prefer the `data` field if present (Soroban emits structured data there)
    let detail = data
        .and_then(|v| v.as_str())
        .unwrap_or(message)
        .to_lowercase();

    if detail.contains("zerohash") || detail.contains("zero_hash") || detail.contains("error(contract, #1)") {
        SorobanError::ZeroHash
    } else if detail.contains("duplicate") || detail.contains("already") || detail.contains("error(contract, #2)") {
        SorobanError::DuplicateHash
    } else if detail.contains("invalidowner") || detail.contains("invalid_owner") || detail.contains("error(contract, #3)") {
        SorobanError::InvalidOwner
    } else if detail.contains("notfound") || detail.contains("not_found") || detail.contains("error(contract, #4)") {
        SorobanError::NotFound
    } else if detail.contains("notipowner") || detail.contains("not_ip_owner") || detail.contains("error(contract, #5)") {
        SorobanError::NotOwner
    } else if detail.contains("iprevoked") || detail.contains("ip_revoked") || detail.contains("error(contract, #6)") {
        SorobanError::Revoked
    } else if detail.contains("notinitialized") || detail.contains("not_initialized") || detail.contains("error(contract, #7)") {
        SorobanError::NotInitialized
    } else {
        SorobanError::ContractError(message.to_string())
    }
}

// ── Simulated / sandboxed invocation result ────────────────────────────────────

/// The parsed result of a `simulateTransaction` call.
#[derive(Debug, Deserialize)]
struct SimulateResult {
    /// XDR-encoded return value (base64).
    #[serde(rename = "xdr")]
    _xdr: Option<String>,
    /// Integer return value, if the contract returns a simple integer.
    #[serde(default)]
    result: Option<serde_json::Value>,
}

// ── Public API ─────────────────────────────────────────────────────────────────

/// Invoke `ip_registry.commit_ip(owner, commitment_hash)` via Soroban RPC.
///
/// Returns the newly assigned `ip_id` on success, or a [`SorobanError`] that
/// HTTP handlers can map to the appropriate HTTP status code.
///
/// # Implementation note
/// The Stellar Soroban RPC `sendTransaction` flow requires:
/// 1. Build a `Transaction` envelope (XDR) via the SDK.
/// 2. Call `simulateTransaction` to get the auth footprint.
/// 3. Apply auth and fee, then call `sendTransaction`.
/// 4. Poll `getTransaction` until status is `SUCCESS` or `FAILED`.
///
/// This function implements a representative version of that flow using the
/// JSON-RPC HTTP interface.  In a real deployment the transaction XDR would be
/// assembled client-side (e.g. using `stellar-sdk` or `soroban-client`); here
/// we use the `invokeContractFunction` shorthand supported by Soroban RPC for
/// testing and tooling purposes.
pub async fn commit_ip(owner: &str, commitment_hash: &str) -> Result<u64, SorobanError> {
    // Basic input validation before hitting the network
    if commitment_hash.len() != 64 {
        // 32-byte hash must be 64 hex chars
        return Err(SorobanError::ZeroHash);
    }
    if commitment_hash.chars().all(|c| c == '0') {
        return Err(SorobanError::ZeroHash);
    }
    if owner.is_empty() {
        return Err(SorobanError::InvalidOwner);
    }

    let client = Client::new();
    let contract_id = ip_registry_contract_id();

    let params = serde_json::json!({
        "transaction": {
            "type": "invoke_contract",
            "contract_id": contract_id,
            "function_name": "commit_ip",
            "args": [
                { "type": "address", "value": owner },
                { "type": "bytes32", "value": commitment_hash }
            ]
        }
    });

    let request = RpcRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "simulateTransaction",
        params,
    };

    let resp = client
        .post(&rpc_url())
        .json(&request)
        .send()
        .await
        .map_err(|e| SorobanError::RpcFailure(e.to_string()))?;

    let body: RpcResponse = resp
        .json()
        .await
        .map_err(|e| SorobanError::RpcFailure(format!("Failed to parse RPC response: {e}")))?;

    if let Some(err) = body.error {
        return Err(classify_error(&err.message, err.data.as_ref()));
    }

    // Extract the integer result (ip_id) from the response
    let result = body.result.ok_or_else(|| {
        SorobanError::RpcFailure("RPC response contained no result".to_string())
    })?;

    // Soroban returns the u64 ip_id as a JSON number or string
    let ip_id = if let Some(n) = result.get("result").and_then(|v| v.as_u64()) {
        n
    } else if let Some(n) = result.as_u64() {
        n
    } else if let Some(s) = result.get("result").and_then(|v| v.as_str()) {
        s.parse::<u64>()
            .map_err(|_| SorobanError::ContractError(format!("Unexpected ip_id format: {s}")))?
    } else {
        return Err(SorobanError::ContractError(format!(
            "Unexpected result shape: {result}"
        )));
    };

    Ok(ip_id)
}
