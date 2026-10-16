//! Regression tests for IP lifecycle guarantees: verification after revoke
//! (#1144), expiry enforcement in verification (#1145), per-IP state on
//! ownership transfer (#1146) and TTL policy (#1147).

extern crate std;

use crate::{IpRegistry, IpRegistryClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Bytes, BytesN, Env};

struct Fixture {
    env: Env,
    client: IpRegistryClient<'static>,
    ip_id: u64,
    secret: BytesN<32>,
    blinding: BytesN<32>,
}

/// Commit an IP whose hash is sha256(secret || blinding), the default
/// scheme that `verify_commitment` recomputes.
fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(IpRegistry, ());
    let client = IpRegistryClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let secret = BytesN::from_array(&env, &[10u8; 32]);
    let blinding = BytesN::from_array(&env, &[20u8; 32]);

    let mut preimage = Bytes::new(&env);
    preimage.append(&Bytes::from(secret.clone()));
    preimage.append(&Bytes::from(blinding.clone()));
    let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

    let ip_id = client.commit_ip(&owner, &commitment_hash, &0u32);

    Fixture {
        env,
        client,
        ip_id,
        secret,
        blinding,
    }
}

// ── #1144: verify_commitment must reject revoked IPs ─────────────────────────

#[test]
fn verify_commitment_accepts_active_ip_with_correct_secret() {
    let f = setup();
    assert!(f.client.verify_commitment(&f.ip_id, &f.secret, &f.blinding));
}

#[test]
fn verify_commitment_rejects_revoked_ip_with_correct_secret() {
    let f = setup();
    f.client.revoke_ip(&f.ip_id);

    // The secret and blinding factor are still correct, but the IP has been
    // revoked, so downstream swap logic must not accept the commitment.
    assert!(!f.client.verify_commitment(&f.ip_id, &f.secret, &f.blinding));
}
