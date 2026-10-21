//! Property tests for the IP revocation invariant (issue #1232).
//!
//! The invariant under test: once an IP has been revoked, no subsequent
//! transfer of that IP may succeed, and verification of that IP must always
//! fail. This module generates random sequences of `Revoke`, `Transfer`, and
//! `Verify` operations over random IPs and owners and asserts the invariant
//! holds after every step.

#![cfg(test)]

use proptest::prelude::*;

use crate::{IpRegistry, IpRegistryClient};

/// A single operation applied to the registry during a property run.
#[derive(Clone, Debug)]
enum Op {
    /// Revoke `ip` on behalf of `owner`.
    Revoke { ip: u32, owner: u32 },
    /// Attempt to transfer `ip` from `from` to `to`.
    Transfer { ip: u32, from: u32, to: u32 },
    /// Attempt to verify `ip` on behalf of `owner`.
    Verify { ip: u32, owner: u32 },
}

/// Strategy producing a random operation over a small IP/owner space so that
/// collisions (and therefore meaningful interleavings) are frequent.
fn op_strategy() -> impl Strategy<Value = Op> {
    let ip = 0u32..8u32;
    let owner = 0u32..4u32;

    prop_oneof![
        (ip.clone(), owner.clone()).prop_map(|(ip, owner)| Op::Revoke { ip, owner }),
        (ip.clone(), owner.clone(), owner.clone())
            .prop_map(|(ip, from, to)| Op::Transfer { ip, from, to }),
        (ip, owner).prop_map(|(ip, owner)| Op::Verify { ip, owner }),
    ]
}

/// Strategy producing a random sequence of operations.
fn ops_strategy() -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(op_strategy(), 0..32)
}

/// Fixed seed used for CI reproducibility.
const FIXED_SEED: u64 = 0x1232_0000_0000_0094;

fn proptest_config() -> ProptestConfig {
    ProptestConfig {
        cases: 256,
        rng_seed: proptest::test_runner::RngSeed::Fixed(FIXED_SEED),
        ..ProptestConfig::default()
    }
}

/// Set up a fresh registry with the given owners registered as participants.
fn setup(owners: &[u32]) -> (IpRegistryClient<'static>, IpRegistry) {
    let env = Box::leak(Box::new(soroban_sdk::Env::default()));
    env.mock_all_auths();
    let contract_id = env.register_contract(None, IpRegistry);
    let client = IpRegistryClient::new(env, &contract_id);
    let registry = IpRegistry;
    for &owner in owners {
        // Register each owner so that transfer/verify calls are well-formed.
        let _ = client.try_register_owner(&owner);
    }
    (client, registry)
}

proptest! {
    #![proptest_config(proptest_config())]

    /// Once an IP is revoked, no subsequent transfer succeeds and verification
    /// always fails.
    #[test]
    fn revoked_ip_cannot_be_transferred_or_verified(ops in ops_strategy()) {
        // Collect the owners referenced by the operations so they can be
        // registered up front.
        let mut owners: Vec<u32> = Vec::new();
        for op in &ops {
            match op {
                Op::Revoke { owner, .. } => owners.push(*owner),
                Op::Transfer { from, to, .. } => {
                    owners.push(*from);
                    owners.push(*to);
                }
                Op::Verify { owner, .. } => owners.push(*owner),
            }
        }
        owners.sort_unstable();
        owners.dedup();

        let (client, _registry) = setup(&owners);

        // Track which IPs have been revoked during this run.
        let mut revoked: Vec<u32> = Vec::new();

        for op in ops {
            match op {
                Op::Revoke { ip, owner } => {
                    // Revocation may fail if the caller is not the owner; only
                    // record the IP as revoked when the call succeeds.
                    if client.try_revoke(&owner, &ip).is_ok() {
                        if !revoked.contains(&ip) {
                            revoked.push(ip);
                        }
                    }
                }
                Op::Transfer { ip, from, to } => {
                    let result = client.try_transfer(&from, &to, &ip);
                    if revoked.contains(&ip) {
                        // Invariant: a revoked IP can never be transferred.
                        prop_assert!(
                            result.is_err(),
                            "transfer of revoked ip {} unexpectedly succeeded",
                            ip
                        );
                    }
                }
                Op::Verify { ip, owner } => {
                    let result = client.try_verify(&owner, &ip);
                    if revoked.contains(&ip) {
                        // Invariant: verification of a revoked IP always fails.
                        prop_assert!(
                            result.is_err(),
                            "verification of revoked ip {} unexpectedly succeeded",
                            ip
                        );
                    }
                }
            }
        }
    }

    /// A revoked IP remains revoked regardless of interleaved operations:
    /// after revocation, every verification attempt fails.
    #[test]
    fn revoked_ip_verification_always_fails(ops in ops_strategy()) {
        let mut owners: Vec<u32> = Vec::new();
        for op in &ops {
            match op {
                Op::Revoke { owner, .. } => owners.push(*owner),
                Op::Transfer { from, to, .. } => {
                    owners.push(*from);
                    owners.push(*to);
                }
                Op::Verify { owner, .. } => owners.push(*owner),
            }
        }
        owners.sort_unstable();
        owners.dedup();

        let (client, _registry) = setup(&owners);

        let mut revoked: Vec<u32> = Vec::new();

        for op in ops {
            match op {
                Op::Revoke { ip, owner } => {
                    if client.try_revoke(&owner, &ip).is_ok() && !revoked.contains(&ip) {
                        revoked.push(ip);
                    }
                }
                Op::Transfer { ip, from, to } => {
                    let _ = client.try_transfer(&from, &to, &ip);
                }
                Op::Verify { ip, owner } => {
                    let result = client.try_verify(&owner, &ip);
                    if revoked.contains(&ip) {
                        prop_assert!(
                            result.is_err(),
                            "verification of revoked ip {} unexpectedly succeeded",
                            ip
                        );
                    }
                }
            }
        }
    }
}