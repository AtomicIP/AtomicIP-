//! Pedersen commitments over Ristretto255 and a non-interactive Schnorr proof
//! of knowledge of a commitment's opening.
//!
//! This is the real zero-knowledge machinery backing the honestly-named
//! `batch_verify_commitments` entry point (see `docs/commitment-scheme.md`).
//! A caller who has committed `secret`/`blinding_factor` as a Pedersen point
//! `commitment = secret·G + blinding_factor·H` (instead of a SHA-256 hash) can
//! prove they know the opening without ever placing `secret` or
//! `blinding_factor` in a transaction argument, event, or storage entry.
//!
//! The proof is a standard Okamoto/Schnorr proof of knowledge of a
//! representation, made non-interactive via Fiat–Shamir:
//!
//! Prover, given `(secret, blinding_factor)` and fresh random nonces
//! `(k_secret, k_blinding)`:
//! 1. `R = k_secret·G + k_blinding·H`
//! 2. `e = H(domain || commitment || R)` (Fiat–Shamir challenge)
//! 3. `s_secret = k_secret + e·secret`, `s_blinding = k_blinding + e·blinding_factor` (mod L)
//!
//! Verifier, given `commitment` and `(R, s_secret, s_blinding)`:
//! - recomputes `e` the same way
//! - accepts iff `s_secret·G + s_blinding·H == R + e·commitment`
//!
//! Nonce generation is the caller's responsibility and happens off-chain;
//! this module only verifies. As with any Schnorr-style proof, a nonce must
//! never be reused across two different messages/commitments or the secret
//! can be recovered.

use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use curve25519_dalek::ristretto::{CompressedRistretto, RistrettoPoint};
use curve25519_dalek::scalar::Scalar;
use soroban_sdk::{Bytes, BytesN, Env};

use crate::HidingCommitmentProof;

/// Domain separator for the Fiat–Shamir challenge hash.
const CHALLENGE_DOMAIN: &[u8; 33] = b"AtomicIP/HidingCommitmentProof/v1";

/// The second Pedersen generator `H`, independent of the Ristretto255
/// basepoint `G`. This is a "nothing up my sleeve" point: nobody (including
/// AtomicIP) knows its discrete log with respect to `G`, because it was
/// derived purely by hashing a fixed domain string to a group element.
///
/// Anyone can independently reproduce this constant:
///
/// ```ignore
/// use curve25519_dalek::ristretto::RistrettoPoint;
/// use sha2::Sha512;
/// let h = RistrettoPoint::hash_from_bytes::<Sha512>(b"AtomicIP/PedersenCommitment/H/v1");
/// assert_eq!(h.compress().to_bytes(), PEDERSEN_H_BYTES);
/// ```
const PEDERSEN_H_BYTES: [u8; 32] = [
    0x24, 0x1a, 0xa9, 0x9e, 0xd6, 0x67, 0xa2, 0x60, 0x0b, 0xea, 0x21, 0xc2, 0xe8, 0x81, 0x46, 0xbe,
    0x67, 0xb3, 0x2a, 0x2b, 0x5c, 0x5e, 0x78, 0x08, 0xb1, 0x56, 0xf7, 0xcc, 0xcf, 0xa2, 0x03, 0x6a,
];

fn pedersen_h() -> RistrettoPoint {
    CompressedRistretto(PEDERSEN_H_BYTES)
        .decompress()
        .expect("PEDERSEN_H_BYTES is a fixed, independently-reproducible valid Ristretto255 point")
}

fn decompress(bytes: &BytesN<32>) -> Option<RistrettoPoint> {
    CompressedRistretto(bytes.to_array()).decompress()
}

fn scalar_from_bytes(bytes: &BytesN<32>) -> Scalar {
    Scalar::from_bytes_mod_order(bytes.to_array())
}

/// Fiat–Shamir challenge `e = H(domain || commitment || r) mod L`.
fn challenge(env: &Env, commitment: &BytesN<32>, r: &BytesN<32>) -> Scalar {
    let mut buf = Bytes::new(env);
    buf.append(&Bytes::from_array(env, CHALLENGE_DOMAIN));
    buf.append(&commitment.clone().into());
    buf.append(&r.clone().into());
    let digest: BytesN<32> = env.crypto().sha256(&buf).into();
    scalar_from_bytes(&digest)
}

/// Verify a Schnorr proof of knowledge of `(secret, blinding_factor)` such
/// that `commitment == secret·G + blinding_factor·H`, without learning
/// either value.
///
/// Returns `false` — never panics — if `commitment` or `proof.r` is not a
/// valid Ristretto255 point encoding, or if the proof does not verify.
pub fn verify_hiding_proof(
    env: &Env,
    commitment: &BytesN<32>,
    proof: &HidingCommitmentProof,
) -> bool {
    let (Some(c), Some(r)) = (decompress(commitment), decompress(&proof.r)) else {
        return false;
    };

    let e = challenge(env, commitment, &proof.r);
    let s_secret = scalar_from_bytes(&proof.s_secret);
    let s_blinding = scalar_from_bytes(&proof.s_blinding);

    let lhs = s_secret * RISTRETTO_BASEPOINT_POINT + s_blinding * pedersen_h();
    let rhs = r + e * c;

    lhs == rhs
}

/// Test-only helpers for constructing Pedersen commitments and hiding proofs,
/// so tests don't need to hand-roll curve arithmetic. Not part of the
/// production contract surface — real provers run off-chain.
#[cfg(test)]
pub mod test_prover {
    use super::*;

    /// `secret·G + blinding_factor·H`, compressed.
    pub fn pedersen_commit(
        env: &Env,
        secret: &BytesN<32>,
        blinding_factor: &BytesN<32>,
    ) -> BytesN<32> {
        let c = scalar_from_bytes(secret) * RISTRETTO_BASEPOINT_POINT
            + scalar_from_bytes(blinding_factor) * pedersen_h();
        BytesN::from_array(env, &c.compress().to_bytes())
    }

    /// Build a valid `HidingCommitmentProof` for `commitment`'s opening,
    /// using the given (test-fixed) nonces. Nonces must be unique per proof
    /// in real usage; tests may reuse fixed nonces freely since they don't
    /// need nonce-reuse resistance, only deterministic reproducibility.
    pub fn prove_hiding(
        env: &Env,
        secret: &BytesN<32>,
        blinding_factor: &BytesN<32>,
        commitment: &BytesN<32>,
        nonce_secret: &BytesN<32>,
        nonce_blinding: &BytesN<32>,
    ) -> HidingCommitmentProof {
        let k_secret = scalar_from_bytes(nonce_secret);
        let k_blinding = scalar_from_bytes(nonce_blinding);
        let r_point = k_secret * RISTRETTO_BASEPOINT_POINT + k_blinding * pedersen_h();
        let r = BytesN::from_array(env, &r_point.compress().to_bytes());

        let e = challenge(env, commitment, &r);
        let s_secret = k_secret + e * scalar_from_bytes(secret);
        let s_blinding = k_blinding + e * scalar_from_bytes(blinding_factor);

        HidingCommitmentProof {
            r,
            s_secret: BytesN::from_array(env, &s_secret.to_bytes()),
            s_blinding: BytesN::from_array(env, &s_blinding.to_bytes()),
        }
    }
}
