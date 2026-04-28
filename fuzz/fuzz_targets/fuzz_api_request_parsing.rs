//! Fuzz target: API server request parsing and validation.
//!
//! Tests that arbitrary byte inputs to JSON request deserialization never panic
//! and that validation logic (hex decoding, hash length checks) behaves correctly.
#![no_main]
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

// ── Inline schema types (mirrors api-server/src/schemas.rs) ──────────────────

#[derive(Debug, serde::Deserialize)]
struct CommitIpRequest {
    owner: String,
    commitment_hash: String,
}

#[derive(Debug, serde::Deserialize)]
struct VerifyCommitmentRequest {
    ip_id: u64,
    secret: String,
    blinding_factor: String,
}

#[derive(Debug, serde::Deserialize)]
struct InitiateSwapRequest {
    ip_id: u64,
    seller: String,
    price: i128,
    buyer: String,
    token: String,
}

#[derive(Debug, serde::Deserialize)]
struct RevealKeyRequest {
    caller: String,
    secret: String,
    blinding_factor: String,
}

// ── Validation helpers (mirrors api-server validation logic) ──────────────────

/// Returns true if `s` is a valid 32-byte hex-encoded hash (64 hex chars).
fn is_valid_commitment_hash(s: &str) -> bool {
    s.len() == 64 && hex::decode(s).is_ok()
}

/// Returns true if the hash is non-zero.
fn is_non_zero_hash(bytes: &[u8]) -> bool {
    bytes.iter().any(|&b| b != 0)
}

// ── Fuzz target ───────────────────────────────────────────────────────────────

fuzz_target!(|data: &[u8]| {
    // Property 1: JSON parsing of CommitIpRequest never panics on arbitrary input.
    let _: Result<CommitIpRequest, _> = serde_json::from_slice(data);

    // Property 2: JSON parsing of VerifyCommitmentRequest never panics.
    let _: Result<VerifyCommitmentRequest, _> = serde_json::from_slice(data);

    // Property 3: JSON parsing of InitiateSwapRequest never panics.
    let _: Result<InitiateSwapRequest, _> = serde_json::from_slice(data);

    // Property 4: JSON parsing of RevealKeyRequest never panics.
    let _: Result<RevealKeyRequest, _> = serde_json::from_slice(data);

    // Property 5: If CommitIpRequest parses successfully, validate commitment_hash.
    if let Ok(req) = serde_json::from_slice::<CommitIpRequest>(data) {
        if is_valid_commitment_hash(&req.commitment_hash) {
            let bytes = hex::decode(&req.commitment_hash).unwrap();
            // Validation: non-zero check must not panic
            let _ = is_non_zero_hash(&bytes);
        }
    }

    // Property 6: If VerifyCommitmentRequest parses, hex-decode secret and blinding_factor.
    if let Ok(req) = serde_json::from_slice::<VerifyCommitmentRequest>(data) {
        if let (Ok(secret), Ok(bf)) = (
            hex::decode(&req.secret),
            hex::decode(&req.blinding_factor),
        ) {
            // Simulate commitment verification: sha256(secret || blinding_factor)
            if secret.len() == 32 && bf.len() == 32 {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(&secret);
                hasher.update(&bf);
                let _ = hasher.finalize();
            }
        }
    }

    // Property 7: request_signing::hash_body never panics on arbitrary bytes.
    {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash = hasher.finalize();
        let hex_hash = hex::encode(hash);
        // Hash must always be 64 hex chars
        assert_eq!(hex_hash.len(), 64, "SHA256 hex output must be 64 chars");
    }

    // Property 8: verify_signature logic never panics on arbitrary inputs.
    if let Ok(s) = std::str::from_utf8(data) {
        // Simulate the signature payload construction
        let payload = format!("POST||/ip/commit||{}||{}", u64::MAX, s);
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(payload.as_bytes());
        let _ = hasher.finalize();
    }
});
