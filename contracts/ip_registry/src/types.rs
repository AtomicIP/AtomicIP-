use soroban_sdk::{contracttype, Address, Bytes, BytesN, Symbol};

// ── TTL ───────────────────────────────────────────────────────────────────────

/// Minimum ledger TTL bump applied to every persistent storage write.
/// ~1 year at ~5s per ledger: 365 * 24 * 3600 / 5 ≈ 6_307_200 ledgers.
#[allow(dead_code)]
pub const LEDGER_BUMP: u32 = 6_307_200;

// ── Event Topics ────────────────────────────────────────────────────────────

pub const REVOKE_TOPIC: Symbol = soroban_sdk::symbol_short!("revoke");
pub const TRANSFER_TOPIC: Symbol = soroban_sdk::symbol_short!("ip_xfer");

// ── Access Control ────────────────────────────────────────────────────────────

/// Access tier constants for tiered IP access control.
/// Tiers are hierarchical: transfer implies verify, verify implies view.
pub const ACCESS_VIEW: u32 = 1;     // Can read IP metadata
pub const ACCESS_VERIFY: u32 = 2;   // Can verify the commitment (view + verify)
pub const ACCESS_TRANSFER: u32 = 3; // Can initiate transfer (view + verify + transfer)

#[contracttype]
#[derive(Clone)]
pub struct IpAccessGrant {
    pub grantee: Address,
    pub access_level: u32, // 1 = view, 2 = verify, 3 = transfer
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
    OwnerCategories(Address), // maps owner -> Vec<BytesN<32>> of category hashes they use
    IpCategories(u64),       // maps ip_id -> Vec<BytesN<32>> of assigned category hashes
    IpLineage(u64),          // stores parent_ip_id for versioning
    IpVersions(u64),         // stores Vec<u64> of all version IDs for a given IP
    IpCommitmentChecksum,    // Issue #346: stores hash of all commitments for rollback protection
    IpAccessGrants(u64),     // Issue #344: stores Vec of (grantee, access_level) for tiered access
    NotarySignature(u64),    // Issue #345: stores notary signature for timestamp notarization
    IpVersionChain(u64),     // stores Vec<u64> of the full version chain rooted at a given IP
    OwnershipChallenge(u64), // Issue #433: stores OwnershipChallenge for a given challenge_id
    NextChallengeId,         // Issue #433: monotonic challenge ID counter
    EncryptionKeyRotation(u64), // Issue #434: stores Vec<BytesN<32>> of old commitment hashes
    MerkleRoot(Address),     // Issue #435: cached Merkle root for an owner's commitment set
    NotaryPublicKey,         // Issue #428: stores the trusted notary Ed25519 public key (32 bytes)
    CommitmentHashes,        // Issue #429: stores Vec<BytesN<32>> of all commitment hashes for rollback protection
    CompressedCommitment(u64), // stores 16-byte compressed commitment for an ip_id
    IpBatchMetadata(u64),    // stores BatchMetadata for an ip_id (Issue #455)
    IpCompression(u64),      // stores CompressionSelection for an ip_id (Issue #456)
    IpEncryptedCommitment(u64), // stores EncryptedCommitmentRecord for an ip_id (Issue #457)
    IpThresholdConfig(u64),  // stores ThresholdConfig for an ip_id (Issue #454)
    IpThresholdSignatures(u64), // stores Vec<ThresholdSignature> for an ip_id (Issue #454)
    IpDisputes(u64),         // stores DisputeRecord for a given dispute_id
    NextDisputeId,           // monotonic dispute ID counter
    IpStake(u64),            // stores StakeRecord for an ip_id
    OwnerReputation(Address), // stores ReputationRecord for an owner
    ArbitratorPool,          // stores Vec<Address> of nominated arbitrators
    ArbitrationCase(u64),    // stores ArbitrationRecord for a given arbitration_id
    NextArbitrationId,       // monotonic arbitration ID counter
    RenewalCount(u64),       // stores renewal count for an ip_id
    DelegateDepth(Address),  // stores delegation depth for a delegate
    Delegates(Address),      // stores Vec<DelegationRecord> for an owner
    ShardIps(u32),           // maps shard_id -> Vec<u64> of IP IDs in that shard
    IpAuditTrail(u64),       // stores Vec<AuditEntry> for an IP
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
    pub parent_ip_id: Option<u64>,       // parent IP ID for versioning
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

/// Issue #433: Challenge record for ownership proof challenge-response.
#[contracttype]
#[derive(Clone)]
pub struct OwnershipChallenge {
    pub challenge_id: u64,
    pub ip_id: u64,
    pub challenger: Address,
    pub nonce: BytesN<32>,
    pub response_hash: Option<BytesN<32>>,
    pub verified: bool,
    pub timestamp: u64,
}
