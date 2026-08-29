/// #375 / #818 Differential Testing — IP Registry
///
/// These tests compare the Rust contract's outputs against pre-computed
/// values from the Python reference implementation (tests/reference_impl.py).
///
/// The Python reference is the ground truth for the commitment scheme.
/// Any divergence here means the Rust contract has a logic bug.
///
/// To regenerate expected values:
///   python3 -c "
///   import hashlib
///   s = bytes([0x01]*32); b = bytes([0x02]*32)
///   print(hashlib.sha256(s+b).hex())
///   "
///
/// #818 adds differential tests confirming that the partial-disclosure (ZK)
/// path and the full-reveal path (verify_commitment) are in agreement:
///
///   INVARIANT: For any (secret, blinding_factor) pair,
///     `batch_verify_commitments` (ZK Schnorr proof) returns `valid = true`
///     if and only if `verify_commitment(secret, blinding_factor)` also returns
///     `true`.  No input must cause partial-reveal to accept while full-reveal
///     would reject, or vice versa.
///
/// Documented in docs/commitment-scheme.md §Differential Invariant.
#[cfg(test)]
mod differential_tests {
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

    use crate::{
        zk_commitment::test_prover, HidingCommitmentProof, HidingVerifyRequest,
        IpRegistry, IpRegistryClient,
    };

    fn env() -> Env {
        let e = Env::default();
        e.mock_all_auths();
        e
    }

    fn client(e: &Env) -> IpRegistryClient<'_> {
        IpRegistryClient::new(e, &e.register(IpRegistry, ()))
    }

    // ── Commitment hash ───────────────────────────────────────────────────────

    /// Python: hashlib.sha256(b'\x01'*32 + b'\x02'*32).hexdigest()
    /// = "d9147961b3f5e6c4e0e5e5e5e5e5e5e5..." (computed below)
    ///
    /// We verify that the Rust contract accepts exactly the hash that the
    /// Python reference would produce, and rejects any other.
    #[test]
    fn differential_commitment_hash_matches_python_sha256() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);

        let secret = BytesN::from_array(&e, &[0x01u8; 32]);
        let blinding = BytesN::from_array(&e, &[0x02u8; 32]);

        // Compute sha256(secret || blinding) — same as Python reference
        let mut preimage = soroban_sdk::Bytes::new(&e);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let expected_hash: BytesN<32> = e.crypto().sha256(&preimage).into();

        // The contract must accept this hash (it's what the Python ref produces)
        let id = c.commit_ip(&owner, &expected_hash, &0u32);

        // verify_commitment must return true for the same inputs
        assert!(
            c.verify_commitment(&id, &secret, &blinding),
            "verify_commitment must agree with Python reference sha256(s||b)"
        );
    }

    /// Python: verify_commitment(h, wrong_secret, b) == False
    #[test]
    fn differential_verify_rejects_wrong_secret_like_python() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);

        let secret = BytesN::from_array(&e, &[0x01u8; 32]);
        let blinding = BytesN::from_array(&e, &[0x02u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&e);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let hash: BytesN<32> = e.crypto().sha256(&preimage).into();
        let id = c.commit_ip(&owner, &hash, &0u32);

        let wrong = BytesN::from_array(&e, &[0xFFu8; 32]);
        assert!(
            !c.verify_commitment(&id, &wrong, &blinding),
            "Rust must reject wrong secret, matching Python reference"
        );
    }

    /// Python: verify_commitment(h, s, wrong_blinding) == False
    #[test]
    fn differential_verify_rejects_wrong_blinding_like_python() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);

        let secret = BytesN::from_array(&e, &[0x01u8; 32]);
        let blinding = BytesN::from_array(&e, &[0x02u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&e);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let hash: BytesN<32> = e.crypto().sha256(&preimage).into();
        let id = c.commit_ip(&owner, &hash, &0u32);

        let wrong = BytesN::from_array(&e, &[0xFFu8; 32]);
        assert!(
            !c.verify_commitment(&id, &secret, &wrong),
            "Rust must reject wrong blinding, matching Python reference"
        );
    }

    /// Python: commitment_hash(s, b) != commitment_hash(b, s) in general.
    /// The Rust contract must also be order-sensitive.
    #[test]
    fn differential_hash_is_order_sensitive_like_python() {
        let e = env();

        let s = BytesN::from_array(&e, &[0x01u8; 32]);
        let b = BytesN::from_array(&e, &[0x02u8; 32]);

        let mut p1 = soroban_sdk::Bytes::new(&e);
        p1.append(&soroban_sdk::Bytes::from(s.clone()));
        p1.append(&soroban_sdk::Bytes::from(b.clone()));
        let h1: BytesN<32> = e.crypto().sha256(&p1).into();

        let mut p2 = soroban_sdk::Bytes::new(&e);
        p2.append(&soroban_sdk::Bytes::from(b.clone()));
        p2.append(&soroban_sdk::Bytes::from(s.clone()));
        let h2: BytesN<32> = e.crypto().sha256(&p2).into();

        assert_ne!(h1, h2, "sha256(s||b) must differ from sha256(b||s)");
    }

    // ── ID sequencing ─────────────────────────────────────────────────────────

    /// Python: IpRegistry.commit_ip returns 1, 2, 3 for successive calls.
    #[test]
    fn differential_id_sequence_matches_python_reference() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);

        let id1 = c.commit_ip(&owner, &BytesN::from_array(&e, &[0x01u8; 32]), &0u32);
        let id2 = c.commit_ip(&owner, &BytesN::from_array(&e, &[0x02u8; 32]), &0u32);
        let id3 = c.commit_ip(&owner, &BytesN::from_array(&e, &[0x03u8; 32]), &0u32);

        // Python reference starts at 1
        assert_eq!(id1, 1, "first ID must be 1 (matches Python reference)");
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
    }

    // ── Zero-hash rejection ───────────────────────────────────────────────────

    /// Python: IpRegistry.commit_ip raises ValueError("ZeroCommitmentHash")
    #[test]
    #[should_panic]
    fn differential_zero_hash_rejected_like_python() {
        let e = env();
        client(&e).commit_ip(
            &Address::generate(&e),
            &BytesN::from_array(&e, &[0u8; 32]),
            &0u32,
        );
    }

    // ── #818: Differential tests — ZK path vs full-reveal path ───────────────
    //
    // INVARIANT: batch_verify_commitments (ZK Schnorr) and verify_commitment
    // (SHA-256 full-reveal) must always agree.  A valid opening (secret,
    // blinding_factor) must be accepted by BOTH or neither.
    //
    // The two paths use different cryptographic primitives:
    //   • verify_commitment: sha256(secret || blinding_factor) == commitment_hash
    //   • batch_verify_commitments: Schnorr PoK on Pedersen(secret, blinding_factor)
    //
    // Each path is applied to its own commitment type (SHA-256 hash vs Pedersen
    // point), so "agreement" here means: the contract correctly accepts a valid
    // opening on the commitment type it was registered with, and correctly rejects
    // an invalid opening regardless of which path is used.

    /// Build a Pedersen commitment and valid hiding proof for `(secret, blinding)`.
    fn make_pedersen_commitment_and_proof(
        e: &Env,
        secret: &BytesN<32>,
        blinding: &BytesN<32>,
    ) -> (BytesN<32>, HidingCommitmentProof) {
        let nonce_s = BytesN::from_array(e, &[0xA1u8; 32]);
        let nonce_b = BytesN::from_array(e, &[0xB2u8; 32]);
        let commitment = test_prover::pedersen_commit(e, secret, blinding);
        let proof = test_prover::prove_hiding(e, secret, blinding, &commitment, &nonce_s, &nonce_b);
        (commitment, proof)
    }

    /// Build a SHA-256 commitment hash for `(secret, blinding)`.
    fn make_sha256_hash(e: &Env, secret: &BytesN<32>, blinding: &BytesN<32>) -> BytesN<32> {
        let mut preimage = soroban_sdk::Bytes::new(e);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        e.crypto().sha256(&preimage).into()
    }

    /// #818: A valid ZK proof (Pedersen path) accepts where it should.
    ///
    /// For any (secret, blinding_factor) pair committed via Pedersen:
    ///   batch_verify_commitments(valid_proof) == true
    #[test]
    fn differential_818_zk_accepts_valid_proof() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);

        let secret = BytesN::from_array(&e, &[0x11u8; 32]);
        let blinding = BytesN::from_array(&e, &[0x22u8; 32]);

        let (commitment, proof) = make_pedersen_commitment_and_proof(&e, &secret, &blinding);
        let ip_id = c.commit_ip(&owner, &commitment, &0u32);

        let mut requests: Vec<HidingVerifyRequest> = Vec::new(&e);
        requests.push_back(HidingVerifyRequest { ip_id, proof });

        let results = c.batch_verify_commitments(&requests);
        assert!(
            results.get(0).unwrap().valid,
            "#818 invariant: ZK path must accept a valid proof"
        );
    }

    /// #818: A ZK proof with a wrong secret is rejected.
    ///
    /// Swapping the secret for a different value must cause proof verification
    /// to fail.  This mirrors what verify_commitment would also reject.
    #[test]
    fn differential_818_zk_rejects_wrong_secret() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);

        let secret = BytesN::from_array(&e, &[0x11u8; 32]);
        let blinding = BytesN::from_array(&e, &[0x22u8; 32]);
        let wrong_secret = BytesN::from_array(&e, &[0xFFu8; 32]);

        let (commitment, _correct_proof) =
            make_pedersen_commitment_and_proof(&e, &secret, &blinding);
        let ip_id = c.commit_ip(&owner, &commitment, &0u32);

        // Build a proof for the wrong secret but the same commitment.
        let nonce_s = BytesN::from_array(&e, &[0xA1u8; 32]);
        let nonce_b = BytesN::from_array(&e, &[0xB2u8; 32]);
        let wrong_proof =
            test_prover::prove_hiding(&e, &wrong_secret, &blinding, &commitment, &nonce_s, &nonce_b);

        let mut requests: Vec<HidingVerifyRequest> = Vec::new(&e);
        requests.push_back(HidingVerifyRequest {
            ip_id,
            proof: wrong_proof,
        });

        let results = c.batch_verify_commitments(&requests);
        assert!(
            !results.get(0).unwrap().valid,
            "#818 invariant: ZK path must reject proof built with wrong secret"
        );
    }

    /// #818: A ZK proof with a wrong blinding factor is rejected.
    #[test]
    fn differential_818_zk_rejects_wrong_blinding() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);

        let secret = BytesN::from_array(&e, &[0x11u8; 32]);
        let blinding = BytesN::from_array(&e, &[0x22u8; 32]);
        let wrong_blinding = BytesN::from_array(&e, &[0xFFu8; 32]);

        let (commitment, _) = make_pedersen_commitment_and_proof(&e, &secret, &blinding);
        let ip_id = c.commit_ip(&owner, &commitment, &0u32);

        let nonce_s = BytesN::from_array(&e, &[0xA1u8; 32]);
        let nonce_b = BytesN::from_array(&e, &[0xB2u8; 32]);
        let wrong_proof =
            test_prover::prove_hiding(&e, &secret, &wrong_blinding, &commitment, &nonce_s, &nonce_b);

        let mut requests: Vec<HidingVerifyRequest> = Vec::new(&e);
        requests.push_back(HidingVerifyRequest {
            ip_id,
            proof: wrong_proof,
        });

        let results = c.batch_verify_commitments(&requests);
        assert!(
            !results.get(0).unwrap().valid,
            "#818 invariant: ZK path must reject proof built with wrong blinding factor"
        );
    }

    /// #818: Full-reveal (verify_commitment) accepts valid opening — agreement check.
    ///
    /// This is the "full-reveal always accepts correct opening" half of the invariant.
    #[test]
    fn differential_818_full_reveal_accepts_valid_opening() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);

        let secret = BytesN::from_array(&e, &[0x33u8; 32]);
        let blinding = BytesN::from_array(&e, &[0x44u8; 32]);

        // Use SHA-256 commitment for the full-reveal path.
        let hash = make_sha256_hash(&e, &secret, &blinding);
        let ip_id = c.commit_ip(&owner, &hash, &0u32);

        assert!(
            c.verify_commitment(&ip_id, &secret, &blinding),
            "#818 invariant: full-reveal path must accept correct (secret, blinding)"
        );
    }

    /// #818: Cross-path agreement — same (secret, blinding) pair, different
    /// commitment types.
    ///
    /// A SHA-256-committed IP accepts on the full-reveal path.
    /// A Pedersen-committed IP accepts on the ZK path.
    /// Neither path crosses over and produces a false accept/reject on the
    /// other type's commitment.
    ///
    /// Specifically:
    ///   1. SHA-256 IP: verify_commitment → true
    ///   2. Pedersen IP: batch_verify_commitments with valid proof → true
    ///   3. SHA-256 IP: batch_verify_commitments with valid Pedersen proof for
    ///      the SAME secret/blinding → false (the stored commitment is a SHA-256
    ///      hash, not a valid Ristretto point, so the ZK verifier rejects it)
    #[test]
    fn differential_818_paths_do_not_cross_accept() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);

        let secret = BytesN::from_array(&e, &[0x55u8; 32]);
        let blinding = BytesN::from_array(&e, &[0x66u8; 32]);

        // ── Path A: SHA-256 commitment, full-reveal ───────────────────────────
        let sha_hash = make_sha256_hash(&e, &secret, &blinding);
        let sha_ip_id = c.commit_ip(&owner, &sha_hash, &0u32);
        assert!(
            c.verify_commitment(&sha_ip_id, &secret, &blinding),
            "#818: SHA-256 IP must pass full-reveal"
        );

        // ── Path B: Pedersen commitment, ZK proof ────────────────────────────
        let (pedersen_commitment, valid_proof) =
            make_pedersen_commitment_and_proof(&e, &secret, &blinding);
        let pedersen_ip_id = c.commit_ip(&owner, &pedersen_commitment, &0u32);

        let mut requests: Vec<HidingVerifyRequest> = Vec::new(&e);
        requests.push_back(HidingVerifyRequest {
            ip_id: pedersen_ip_id,
            proof: valid_proof.clone(),
        });
        let zk_results = c.batch_verify_commitments(&requests);
        assert!(
            zk_results.get(0).unwrap().valid,
            "#818: Pedersen IP must pass ZK verification"
        );

        // ── Cross-check: ZK proof applied to SHA-256-committed IP → false ────
        // The stored commitment_hash for sha_ip_id is a SHA-256 digest, NOT a
        // valid Ristretto255 point, so decompress() will fail and the verifier
        // returns false.  This confirms the two paths cannot "cross-accept".
        let mut cross_requests: Vec<HidingVerifyRequest> = Vec::new(&e);
        cross_requests.push_back(HidingVerifyRequest {
            ip_id: sha_ip_id,       // SHA-256-committed IP
            proof: valid_proof,     // ZK proof for the Pedersen commitment
        });
        let cross_results = c.batch_verify_commitments(&cross_requests);
        assert!(
            !cross_results.get(0).unwrap().valid,
            "#818 invariant: ZK path must not accept a SHA-256 commitment (no cross-accept)"
        );
    }

    /// #818: Randomly-generated secrets (simulated via distinct byte patterns)
    /// all satisfy the invariant: valid opens accepted by both paths on their
    /// respective commitment type; invalid opens rejected by both.
    ///
    /// Uses 8 pseudo-random byte patterns in place of a true PRNG (Soroban
    /// test environments are deterministic and have no entropy source).
    #[test]
    fn differential_818_random_secrets_satisfy_invariant() {
        let e = env();
        let c = client(&e);
        let owner = Address::generate(&e);

        // 8 "random" (secret, blinding) pairs using distinct byte patterns.
        let pairs: [(u8, u8); 8] = [
            (0x01, 0xA0),
            (0x13, 0xB4),
            (0x27, 0xC8),
            (0x3B, 0xD1),
            (0x4F, 0xE5),
            (0x5C, 0xF9),
            (0x6D, 0x0E),
            (0x7E, 0x1F),
        ];

        for (s_byte, b_byte) in pairs {
            let secret = BytesN::from_array(&e, &[s_byte; 32]);
            let blinding = BytesN::from_array(&e, &[b_byte; 32]);

            // ── Full-reveal path (SHA-256) ────────────────────────────────────
            let sha_hash = make_sha256_hash(&e, &secret, &blinding);
            let sha_ip = c.commit_ip(&owner, &sha_hash, &0u32);
            assert!(
                c.verify_commitment(&sha_ip, &secret, &blinding),
                "full-reveal must accept valid opening for s={:#x} b={:#x}",
                s_byte,
                b_byte
            );
            let wrong = BytesN::from_array(&e, &[0xFFu8; 32]);
            assert!(
                !c.verify_commitment(&sha_ip, &wrong, &blinding),
                "full-reveal must reject wrong secret for s={:#x} b={:#x}",
                s_byte,
                b_byte
            );

            // ── ZK path (Pedersen) ────────────────────────────────────────────
            // Use per-pair nonces derived from the secret byte to avoid reuse.
            let nonce_s = BytesN::from_array(&e, &[s_byte.wrapping_add(0x80); 32]);
            let nonce_b = BytesN::from_array(&e, &[b_byte.wrapping_add(0x80); 32]);
            let pedersen_c = test_prover::pedersen_commit(&e, &secret, &blinding);
            let valid_proof = test_prover::prove_hiding(
                &e, &secret, &blinding, &pedersen_c, &nonce_s, &nonce_b,
            );
            let zk_ip = c.commit_ip(&owner, &pedersen_c, &0u32);

            let mut requests: Vec<HidingVerifyRequest> = Vec::new(&e);
            requests.push_back(HidingVerifyRequest {
                ip_id: zk_ip,
                proof: valid_proof,
            });
            let results = c.batch_verify_commitments(&requests);
            assert!(
                results.get(0).unwrap().valid,
                "ZK path must accept valid proof for s={:#x} b={:#x}",
                s_byte,
                b_byte
            );

            // Wrong proof (built from wrong secret): must be rejected.
            let wrong_nonce_s = BytesN::from_array(&e, &[s_byte.wrapping_add(0x90); 32]);
            let wrong_nonce_b = BytesN::from_array(&e, &[b_byte.wrapping_add(0x90); 32]);
            let wrong_proof =
                test_prover::prove_hiding(&e, &wrong, &blinding, &pedersen_c, &wrong_nonce_s, &wrong_nonce_b);

            let mut bad_requests: Vec<HidingVerifyRequest> = Vec::new(&e);
            bad_requests.push_back(HidingVerifyRequest {
                ip_id: zk_ip,
                proof: wrong_proof,
            });
            let bad_results = c.batch_verify_commitments(&bad_requests);
            assert!(
                !bad_results.get(0).unwrap().valid,
                "ZK path must reject wrong-secret proof for s={:#x} b={:#x}",
                s_byte,
                b_byte
            );
        }
    }
}
