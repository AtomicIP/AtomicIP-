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

// ── #1146: per-IP authority state does not survive ownership transfer ────────

#[test]
fn transfer_clears_access_grants() {
    let f = setup();
    let grantee = Address::generate(&f.env);
    let new_owner = Address::generate(&f.env);

    f.client.grant_ip_access(&f.ip_id, &grantee, &3u32);
    assert!(f.client.check_ip_access(&f.ip_id, &grantee, &1u32));

    f.client.transfer_ip(&f.ip_id, &new_owner);

    assert_eq!(f.client.get_ip_access_grants(&f.ip_id).len(), 0);
    assert!(!f.client.check_ip_access(&f.ip_id, &grantee, &1u32));
}

#[test]
fn transfer_clears_licenses() {
    let f = setup();
    let licensee = Address::generate(&f.env);
    let new_owner = Address::generate(&f.env);
    let terms = BytesN::from_array(&f.env, &[7u8; 32]);

    f.client.grant_license(&f.ip_id, &licensee, &terms);
    assert_eq!(f.client.get_licenses(&f.ip_id).len(), 1);

    f.client.transfer_ip(&f.ip_id, &new_owner);

    assert_eq!(f.client.get_licenses(&f.ip_id).len(), 0);
}

#[test]
fn transfer_resets_co_owners_and_cap_table() {
    let f = setup();
    let co_owner = Address::generate(&f.env);
    let new_owner = Address::generate(&f.env);

    f.client.add_co_owner(&f.ip_id, &co_owner, &20u32);
    assert_eq!(f.client.get_ip(&f.ip_id).co_owners.len(), 1);

    f.client.transfer_ip(&f.ip_id, &new_owner);

    assert_eq!(f.client.get_ip(&f.ip_id).co_owners.len(), 0);
    assert_eq!(f.client.get_ownership_shares(&f.ip_id).len(), 0);
}

#[test]
fn transfer_clears_threshold_configuration() {
    let f = setup();
    let signer = Address::generate(&f.env);
    let new_owner = Address::generate(&f.env);
    let signers = soroban_sdk::Vec::from_array(&f.env, [signer]);

    f.client.require_threshold_signatures(&f.ip_id, &1u32, &signers);
    assert!(f.client.get_threshold_config(&f.ip_id).is_some());

    f.client.transfer_ip(&f.ip_id, &new_owner);

    assert!(f.client.get_threshold_config(&f.ip_id).is_none());
}

#[test]
fn transfer_retains_audit_history() {
    let f = setup();
    let new_owner = Address::generate(&f.env);
    let before = f.client.get_ip_audit_trail(&f.ip_id).len();

    f.client.transfer_ip(&f.ip_id, &new_owner);

    // History is retained and gains an entry for the transfer.
    assert_eq!(f.client.get_ip_audit_trail(&f.ip_id).len(), before + 1);
}

#[test]
fn new_owner_can_grant_access_after_transfer() {
    let f = setup();
    let grantee = Address::generate(&f.env);
    let new_owner = Address::generate(&f.env);

    f.client.transfer_ip(&f.ip_id, &new_owner);
    f.client.grant_ip_access(&f.ip_id, &grantee, &1u32);

    assert!(f.client.check_ip_access(&f.ip_id, &grantee, &1u32));
}
