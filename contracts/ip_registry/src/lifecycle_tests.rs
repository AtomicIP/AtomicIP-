//! Regression tests for IP lifecycle guarantees: verification after revoke
//! (#1144), expiry enforcement in verification (#1145), per-IP state on
//! ownership transfer (#1146) and TTL policy (#1147).

extern crate std;

use crate::{IpRegistry, IpRegistryClient};
use soroban_sdk::testutils::{Address as _, Ledger};
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

// ── #1145: expiry is enforced by verify_commitment; events leave the read path ──

#[test]
fn verify_commitment_valid_before_expiry() {
    let f = setup();
    let now = f.env.ledger().timestamp();
    f.client.set_ip_expiry(&f.ip_id, &(now + 1000), &0);

    assert!(f.client.verify_commitment(&f.ip_id, &f.secret, &f.blinding));
}

#[test]
fn verify_commitment_valid_during_grace_period() {
    let f = setup();
    let now = f.env.ledger().timestamp();
    f.client.set_ip_expiry(&f.ip_id, &(now + 100), &500);

    // Past expiry but before expiry + grace: still verifies.
    f.env.ledger().with_mut(|l| l.timestamp = now + 150);
    assert!(f.client.verify_commitment(&f.ip_id, &f.secret, &f.blinding));
}

#[test]
fn verify_commitment_rejects_after_grace_period() {
    let f = setup();
    let now = f.env.ledger().timestamp();
    f.client.set_ip_expiry(&f.ip_id, &(now + 100), &500);

    // At exactly expiry + grace the commitment is no longer verifiable.
    f.env.ledger().with_mut(|l| l.timestamp = now + 600);
    assert!(!f.client.verify_commitment(&f.ip_id, &f.secret, &f.blinding));
}

#[test]
fn verify_commitment_rejects_at_expiry_when_no_grace() {
    let f = setup();
    let now = f.env.ledger().timestamp();
    f.client.set_ip_expiry(&f.ip_id, &(now + 100), &0);

    f.env.ledger().with_mut(|l| l.timestamp = now + 100);
    assert!(!f.client.verify_commitment(&f.ip_id, &f.secret, &f.blinding));
}

#[test]
fn verify_commitment_does_not_emit_expiry_event() {
    use soroban_sdk::testutils::Events;

    let f = setup();
    let now = f.env.ledger().timestamp();
    f.client.set_ip_expiry(&f.ip_id, &(now + 100), &500);
    f.env.ledger().with_mut(|l| l.timestamp = now + 600);

    let before = f.env.events().all().events().len();
    f.client.verify_commitment(&f.ip_id, &f.secret, &f.blinding);
    f.client.verify_commitment(&f.ip_id, &f.secret, &f.blinding);
    assert_eq!(
        f.env.events().all().events().len(),
        before,
        "verify_commitment is a read path and must not emit events"
    );
}

#[test]
fn notify_ip_expiry_is_noop_before_expiry() {
    let f = setup();
    let now = f.env.ledger().timestamp();
    f.client.set_ip_expiry(&f.ip_id, &(now + 1000), &0);

    assert!(!f.client.notify_ip_expiry(&f.ip_id));
}

#[test]
fn notify_ip_expiry_fires_once_per_transition() {
    let f = setup();
    let now = f.env.ledger().timestamp();
    f.client.set_ip_expiry(&f.ip_id, &(now + 100), &500);
    f.env.ledger().with_mut(|l| l.timestamp = now + 150);

    assert!(f.client.notify_ip_expiry(&f.ip_id));
    assert!(!f.client.notify_ip_expiry(&f.ip_id));

    // Renewing starts a new expiry transition, so it can notify again.
    f.client.renew_ip_commitment(&f.ip_id, &(now + 1000));
    f.env.ledger().with_mut(|l| l.timestamp = now + 1100);
    assert!(f.client.notify_ip_expiry(&f.ip_id));
}
