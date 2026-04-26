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
    PartialDisclosure(u64), // stores partial_hash for a given ip_id after reveal
    IpLicenses(u64),        // stores license entries for a given ip_id
    CategoryIps(Bytes),     // maps category -> Vec<u64> of IP IDs
    PowDifficulty,          // stores the current PoW difficulty (leading zero bits required)
    IpDelegates(Address),   // stores Vec<Address> of delegates for an owner
    SuggestedPrice(u64),    // stores suggested price for an IP
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
    pub expiry_timestamp: u64,   // 0 = no expiry
    pub metadata: Bytes,         // max 1 KB; empty = no metadata
    pub priority: u8,            // 0-10 scale, 0 = no priority, 10 = highest
    pub category: Bytes,         // category for organizing IPs
    pub commitment_strength: u8, // 0-100 scale for cryptographic strength
    pub co_owners: Vec<Address>,
}

#[contracttype]
#[derive(Clone)]
pub struct LicenseEntry {
    pub licensee: Address,
    pub terms_hash: BytesN<32>,
}
