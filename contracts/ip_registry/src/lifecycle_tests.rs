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

// ── #1147: TTL policy — extend_ttl must use LEDGER_BUMP, never a literal ─────

fn is_numeric_literal(arg: &str) -> bool {
    !arg.is_empty() && arg.chars().all(|c| c.is_ascii_digit() || c == '_')
}

/// Returns `(line, call)` for every `extend_ttl(...)` call in `src` whose last
/// two arguments (the threshold and extend-to TTLs) are numeric literals.
fn literal_ttl_calls(src: &str) -> std::vec::Vec<(usize, std::string::String)> {
    const CALL: &str = "extend_ttl(";
    let mut found = std::vec::Vec::new();

    for (start, _) in src.match_indices(CALL) {
        let args_start = start + CALL.len();

        // Find the matching closing parenthesis.
        let mut depth = 1usize;
        let mut end = args_start;
        for (k, c) in src[args_start..].char_indices() {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = args_start + k;
                        break;
                    }
                }
                _ => {}
            }
        }

        // Split the argument list on top-level commas only.
        let mut args = std::vec::Vec::new();
        let mut nest = 0usize;
        let mut last = args_start;
        for (k, c) in src[args_start..end].char_indices() {
            match c {
                '(' | '[' => nest += 1,
                ')' | ']' => nest -= 1,
                ',' if nest == 0 => {
                    args.push(src[last..args_start + k].trim());
                    last = args_start + k + 1;
                }
                _ => {}
            }
        }
        args.push(src[last..end].trim());
        let args: std::vec::Vec<&str> = args.into_iter().filter(|a| !a.is_empty()).collect();

        if args.len() >= 3 && args[args.len() - 2..].iter().all(|a| is_numeric_literal(a)) {
            let line = src[..start].matches('\n').count() + 1;
            found.push((line, std::format!("extend_ttl({})", args.join(", "))));
        }
    }

    found
}

#[test]
fn literal_ttl_scanner_detects_literal_values() {
    let bad = "env.storage().persistent().extend_ttl(&key, 50000, 50000);";
    let good = "env.storage().persistent().extend_ttl(&key, LEDGER_BUMP, LEDGER_BUMP);";
    let multiline = "extend_ttl(\n    &DataKey::OwnerIps(owner.clone()),\n    6_307_200,\n    6_307_200,\n);";
    assert_eq!(literal_ttl_calls(bad).len(), 1);
    assert_eq!(literal_ttl_calls(multiline).len(), 1);
    assert!(literal_ttl_calls(good).is_empty());
}

#[test]
fn extend_ttl_uses_ledger_bump_policy() {
    let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = std::vec::Vec::new();

    for entry in std::fs::read_dir(&src_dir).expect("read src directory") {
        let path = entry.expect("directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        // This file contains the scanner's own sample strings.
        if path.file_name().and_then(|n| n.to_str()) == Some("lifecycle_tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read source file");
        for (line, call) in literal_ttl_calls(&text) {
            violations.push(std::format!("{}:{}: {}", path.display(), line, call));
        }
    }

    assert!(
        violations.is_empty(),
        "extend_ttl must use LEDGER_BUMP instead of literal TTLs:\n{}",
        violations.join("\n")
    );
}
