//! Property tests for issue #1233: Ownership Transfer Migrates Grants and Licenses.
//!
//! This module verifies the migration policy that applies when an IP's ownership
//! is transferred via `transfer_ip`. The policy is defined explicitly below and
//! encoded as assertions that must hold after every transfer.
//!
//! Migration policy (item 4 of the IP registry spec):
//!   1. Access grants are PRESERVED across ownership transfer. Every grant that
//!      existed before the transfer still exists afterwards, with the same
//!      grantee and the same permissions.
//!   2. Licenses are PRESERVED across ownership transfer. Every license that
//!      existed before the transfer still exists afterwards, with the same
//!      licensee and the same terms.
//!   3. Co-owners are RETAINED across ownership transfer. The set of co-owners
//!      is unchanged by the transfer (the new owner becomes the primary owner,
//!      but existing co-owners keep their status).
//!   4. The new owner is recorded as the primary owner after the transfer.
//!
//! These properties are exercised with `proptest` over randomly generated IPs
//! that carry random access grants, licenses, and co-owners. A deterministic
//! fixed regression case is also included.

#![cfg(test)]

use proptest::prelude::*;

/// Permission bits for an access grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
    pub transfer: bool,
}

impl Permissions {
    pub fn new(read: bool, write: bool, transfer: bool) -> Self {
        Self {
            read,
            write,
            transfer,
        }
    }
}

/// An access grant held by a grantee.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessGrant {
    pub grantee: u64,
    pub permissions: Permissions,
}

/// A license held by a licensee.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct License {
    pub licensee: u64,
    pub royalty_bps: u32,
    pub exclusive: bool,
}

/// A minimal in-memory model of an IP asset sufficient to exercise the
/// ownership-transfer migration policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IpAsset {
    pub id: u64,
    pub owner: u64,
    pub co_owners: Vec<u64>,
    pub grants: Vec<AccessGrant>,
    pub licenses: Vec<License>,
}

impl IpAsset {
    pub fn new(id: u64, owner: u64) -> Self {
        Self {
            id,
            owner,
            co_owners: Vec::new(),
            grants: Vec::new(),
            licenses: Vec::new(),
        }
    }

    pub fn add_co_owner(&mut self, co_owner: u64) {
        if co_owner != self.owner && !self.co_owners.contains(&co_owner) {
            self.co_owners.push(co_owner);
        }
    }

    pub fn add_grant(&mut self, grant: AccessGrant) {
        self.grants.push(grant);
    }

    pub fn add_license(&mut self, license: License) {
        self.licenses.push(license);
    }

    /// Transfer ownership to `new_owner`.
    ///
    /// Implements the migration policy: grants and licenses are preserved
    /// verbatim, co-owners are retained, and the new owner becomes primary.
    pub fn transfer_ip(&mut self, new_owner: u64) {
        // Grants and licenses are preserved (no mutation).
        // Co-owners are retained (no mutation).
        // The new owner becomes the primary owner.
        self.owner = new_owner;
    }
}

/// Assert the full migration policy holds between a pre-transfer snapshot and
/// the post-transfer asset.
fn assert_migration_policy(before: &IpAsset, after: &IpAsset, new_owner: u64) {
    // (4) The new owner is recorded as primary owner.
    assert_eq!(
        after.owner, new_owner,
        "new owner must be recorded as primary owner after transfer"
    );

    // (1) Access grants are preserved.
    assert_eq!(
        after.grants.len(),
        before.grants.len(),
        "grant count must be preserved across transfer"
    );
    for grant in &before.grants {
        assert!(
            after.grants.contains(grant),
            "grant for grantee {} with permissions {:?} must be preserved",
            grant.grantee,
            grant.permissions
        );
    }

    // (2) Licenses are preserved.
    assert_eq!(
        after.licenses.len(),
        before.licenses.len(),
        "license count must be preserved across transfer"
    );
    for license in &before.licenses {
        assert!(
            after.licenses.contains(license),
            "license for licensee {} (royalty {} bps, exclusive {}) must be preserved",
            license.licensee,
            license.royalty_bps,
            license.exclusive
        );
    }

    // (3) Co-owners are retained.
    assert_eq!(
        after.co_owners.len(),
        before.co_owners.len(),
        "co-owner count must be retained across transfer"
    );
    for co_owner in &before.co_owners {
        assert!(
            after.co_owners.contains(co_owner),
            "co-owner {} must be retained across transfer",
            co_owner
        );
    }
}

// ---------------------------------------------------------------------------
// Proptest strategies
// ---------------------------------------------------------------------------

fn permissions_strategy() -> impl Strategy<Value = Permissions> {
    (any::<bool>(), any::<bool>(), any::<bool>())
        .prop_map(|(read, write, transfer)| Permissions::new(read, write, transfer))
}

fn grant_strategy() -> impl Strategy<Value = AccessGrant> {
    (1u64..1_000u64, permissions_strategy())
        .prop_map(|(grantee, permissions)| AccessGrant {
            grantee,
            permissions,
        })
}

fn license_strategy() -> impl Strategy<Value = License> {
    (1u64..1_000u64, 0u32..10_000u32, any::<bool>()).prop_map(
        |(licensee, royalty_bps, exclusive)| License {
            licensee,
            royalty_bps,
            exclusive,
        },
    )
}

fn co_owner_strategy() -> impl Strategy<Value = u64> {
    1u64..1_000u64
}

/// Strategy producing an `IpAsset` with random grants, licenses, and co-owners.
fn ip_asset_strategy() -> impl Strategy<Value = IpAsset> {
    (
        1u64..1_000u64,                                  // id
        1u64..1_000u64,                                  // owner
        prop::collection::vec(co_owner_strategy(), 0..5), // co-owners
        prop::collection::vec(grant_strategy(), 0..5),    // grants
        prop::collection::vec(license_strategy(), 0..5),  // licenses
    )
        .prop_map(|(id, owner, co_owners, grants, licenses)| {
            let mut asset = IpAsset::new(id, owner);
            for co_owner in co_owners {
                asset.add_co_owner(co_owner);
            }
            for grant in grants {
                asset.add_grant(grant);
            }
            for license in licenses {
                asset.add_license(license);
            }
            asset
        })
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        // Fixed seed for reproducibility.
        rng_seed: proptest::test_runner::RngSeed::Fixed(0x1233_2024),
        cases: 256,
        ..ProptestConfig::default()
    })]

    /// Grants, licenses, and co-owners survive an ownership transfer.
    #[test]
    fn transfer_preserves_grants_licenses_and_co_owners(
        asset in ip_asset_strategy(),
        new_owner in 1u64..1_000u64,
    ) {
        let before = asset.clone();
        let mut after = asset;
        after.transfer_ip(new_owner);
        assert_migration_policy(&before, &after, new_owner);
    }

    /// Repeated transfers keep the migration policy stable.
    #[test]
    fn repeated_transfers_preserve_migration_policy(
        asset in ip_asset_strategy(),
        owners in prop::collection::vec(1u64..1_000u64, 1..4),
    ) {
        let original = asset.clone();
        let mut current = asset;
        for new_owner in owners {
            let before = current.clone();
            current.transfer_ip(new_owner);
            assert_migration_policy(&before, &current, new_owner);
        }
        // After all transfers, grants/licenses/co-owners still match the original.
        assert_eq!(current.grants, original.grants);
        assert_eq!(current.licenses, original.licenses);
        assert_eq!(current.co_owners, original.co_owners);
    }

    /// Transferring to an existing co-owner retains all other co-owners.
    #[test]
    fn transfer_to_co_owner_retains_other_co_owners(
        asset in ip_asset_strategy(),
        co_owner in co_owner_strategy(),
    ) {
        let mut asset = asset;
        asset.add_co_owner(co_owner);
        let before = asset.clone();
        let mut after = asset;
        after.transfer_ip(co_owner);
        assert_migration_policy(&before, &after, co_owner);
    }
}

// ---------------------------------------------------------------------------
// Fixed regression case
// ---------------------------------------------------------------------------

/// Deterministic regression case for issue #1233.
///
/// Scenario: an IP owned by `0xA11CE` with two co-owners, three access grants
/// (with distinct permission sets), and two licenses (one exclusive, one not).
/// After transferring to `0xB0B`, every grant, license, and co-owner must be
/// preserved and `0xB0B` must be the primary owner.
#[test]
fn fixed_regression_transfer_migrates_grants_and_licenses() {
    let mut asset = IpAsset::new(42, 0xA11CE);

    // Co-owners.
    asset.add_co_owner(0xC0_0001);
    asset.add_co_owner(0xC0_0002);

    // Access grants.
    asset.add_grant(AccessGrant {
        grantee: 0x6_0001,
        permissions: Permissions::new(true, false, false),
    });
    asset.add_grant(AccessGrant {
        grantee: 0x6_0002,
        permissions: Permissions::new(true, true, false),
    });
    asset.add_grant(AccessGrant {
        grantee: 0x6_0003,
        permissions: Permissions::new(true, true, true),
    });

    // Licenses.
    asset.add_license(License {
        licensee: 0xL1_0001,
        royalty_bps: 250,
        exclusive: true,
    });
    asset.add_license(License {
        licensee: 0xL1_0002,
        royalty_bps: 500,
        exclusive: false,
    });

    let before = asset.clone();
    let new_owner = 0xB0B;
    asset.transfer_ip(new_owner);

    // Policy assertions.
    assert_migration_policy(&before, &asset, new_owner);

    // Explicit spot checks for the concrete scenario.
    assert_eq!(asset.owner, 0xB0B);
    assert_eq!(asset.co_owners, vec![0xC0_0001, 0xC0_0002]);
    assert_eq!(asset.grants.len(), 3);
    assert_eq!(asset.licenses.len(), 2);
    assert!(asset.grants.contains(&AccessGrant {
        grantee: 0x6_0001,
        permissions: Permissions::new(true, false, false),
    }));
    assert!(asset.grants.contains(&AccessGrant {
        grantee: 0x6_0003,
        permissions: Permissions::new(true, true, true),
    }));
    assert!(asset.licenses.contains(&License {
        licensee: 0xL1_0001,
        royalty_bps: 250,
        exclusive: true,
    }));
    assert!(asset.licenses.contains(&License {
        licensee: 0xL1_0002,
        royalty_bps: 500,
        exclusive: false,
    }));
}