use soroban_sdk::{contracttype, Address, BytesN, Symbol, Vec, Env, Bytes};

// ── TTL ───────────────────────────────────────────────────────────────────────

/// Minimum ledger TTL bump applied to every persistent storage write.
/// ~1 year at ~5s per ledger: 365 * 24 * 3600 / 5 ≈ 6_307_200 ledgers.
pub const LEDGER_BUMP: u32 = 6_307_200;

// ── Event Topics ────────────────────────────────────────────────────────────

pub const REVOKE_TOPIC: Symbol = soroban_sdk::symbol_short!("revoke");

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
    /// #348: Maps ip_id → Vec<(Address, u32)> for fractional ownership (address, percentage)
    IpOwners(u64),
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
