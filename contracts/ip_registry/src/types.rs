use soroban_sdk::{contracttype, Address, BytesN, Symbol, Bytes};

// ── TTL ───────────────────────────────────────────────────────────────────────

/// Minimum ledger TTL bump applied to every persistent storage write.
/// ~1 year at ~5s per ledger: 365 * 24 * 3600 / 5 ≈ 6_307_200 ledgers.
#[allow(dead_code)]
pub const LEDGER_BUMP: u32 = 6_307_200;

// ── Event Topics ────────────────────────────────────────────────────────────

pub const REVOKE_TOPIC: Symbol = soroban_sdk::symbol_short!("revoke");
pub const TRANSFER_TOPIC: Symbol = soroban_sdk::symbol_short!("ip_xfer");

// ── Access Control ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct IpAccessGrant {
    pub grantee: Address,
    pub access_level: u32, // 0 = none, 1 = read-only, 2 = read-write
}

// ── Storage Keys ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Debug, PartialEq)]
pub enum DataKey {
    IpRecord(u64),
    OwnerIps(Address),
    NextId,
    CommitmentOwner(BytesN<32>), // tracks which owner already holds a commitment hash
    Admin,
    CategoryIps(BytesN<32>), // maps category hash -> Vec<u64> of IP IDs
    IpLineage(u64), // stores parent_ip_id for versioning
    IpVersions(u64), // stores Vec<u64> of all version IDs for a given IP
    IpCommitmentChecksum, // Issue #346: stores hash of all commitments for rollback protection
    IpAccessGrants(u64), // Issue #344: stores Vec of (grantee, access_level) for tiered access
    NotarySignature(u64), // Issue #345: stores notary signature for timestamp notarization
    IpVersionChain(u64), // stores Vec<u64> of the full version chain rooted at a given IP
}

// ── Types ────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct IpRecord {
    pub ip_id: u64,
    pub owner: Address,
    pub commitment_hash: BytesN<32>,
    pub timestamp: u64,
    pub revoked: bool,
    pub co_owners: soroban_sdk::Vec<Address>,
    pub parent_ip_id: Option<u64>, // parent IP ID for versioning
    pub notary_signature: Option<Bytes>, // Issue #345: notary signature for timestamp notarization
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OwnershipShare {
    pub address: Address,
    pub percentage: u32, // 0-100, sum of all should be 100
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CoOwnerAddedEvent {
    pub ip_id: u64,
    pub co_owner: Address,
    pub percentage: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CoOwnerRemovedEvent {
    pub ip_id: u64,
    pub co_owner: Address,
}
