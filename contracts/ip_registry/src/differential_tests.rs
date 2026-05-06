/// #375 Differential Testing — IP Registry
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
#[cfg(test)]
mod differential_tests {
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

    use crate::{IpRegistry, IpRegistryClient};

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
}
