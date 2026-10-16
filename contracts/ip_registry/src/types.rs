use soroban_sdk::{contracttype, Address, Bytes, BytesN, Symbol};

// ── TTL ───────────────────────────────────────────────────────────────────────

/// Minimum ledger TTL bump applied to every persistent storage write.
/// ~1 year at ~5s per ledger: 365 * 24 * 3600 / 5 ≈ 6_307_200 ledgers.
#[allow(dead_code)]
pub const LEDGER_BUMP: u32 = 6_307_200;

// ── Event Topics ────────────────────────────────────────────────────────────

pub const REVOKE_TOPIC: Symbol = soroban_sdk::symbol_short!("revoke");
pub const TRANSFER_TOPIC: Symbol = soroban_sdk::symbol_short!("ip_xfer");
pub const BATCH_VERIFY_TOPIC: Symbol = soroban_sdk::symbol_short!("batch_vfy");
pub const EXPIRY_TOPIC: Symbol = soroban_sdk::symbol_short!("ip_expiry");

// ── Access Control ────────────────────────────────────────────────────────────

/// Access tier constants for tiered IP access control.
/// Tiers are hierarchical: transfer implies verify, verify implies view.
#[allow(dead_code)]
pub const ACCESS_VIEW: u32 = 1; // Can read IP metadata
#[allow(dead_code)]
pub const ACCESS_VERIFY: u32 = 2; // Can verify the commitment (view + verify)
#[allow(dead_code)]
pub const ACCESS_TRANSFER: u32 = 3; // Can initiate transfer (view + verify + transfer)

#[contracttype]
#[derive(Clone)]
pub struct IpAccessGrant {
    pub grantee: Address,
    pub access_level: u32, // 1 = view, 2 = verify, 3 = transfer
}

/// Cached proof path tied to the owner's current Merkle root.
#[contracttype]
#[derive(Clone)]
pub struct MerkleProofCache {
    pub root: BytesN<32>,
    pub proof: soroban_sdk::Vec<BytesN<32>>,
}

/// Hash function used to derive a commitment from secret material.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitmentAlgorithm {
    Pedersen,
    Sha256,
    Blake3,
}

// ── Storage Keys ────────────────────────────────────────────────────────────


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
    pub expiry_timestamp: u64,           // 0 = no expiry
    pub grace_period_seconds: u64,       // seconds after expiry before permanent deletion
    pub unlock_time: u64,                // #975: time-lock release timestamp, 0 = no time-lock
    pub privacy_level: u32,              // #977: 0=Public, 1=Private, 2=Restricted, 3=Confidential
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
    /// Unix timestamp (seconds) after which this challenge is considered expired.
    /// Computed as `timestamp + challenge_ttl_seconds` at creation time.
    pub expires_at: u64,
}

/// Issue #973: Encrypted metadata for commitment discovery and discoverability.
/// Stores encrypted title, description, and searchable tags for each IP commitment.
/// The metadata is encrypted to preserve privacy while enabling search.
#[contracttype]
#[derive(Clone)]
pub struct IpMetadata {
    pub ip_id: u64,
    /// Encrypted bytes containing metadata (title, description, tags)
    /// Format: AES-GCM encrypted JSON with nonce prepended
    pub encrypted_data: Bytes,
    /// Encryption nonce used for this metadata
    pub nonce: BytesN<12>,
    /// Timestamp when metadata was created/updated
    pub timestamp: u64,
    /// Tags for search indexing (hashes of tag strings)
    pub tag_hashes: soroban_sdk::Vec<BytesN<32>>,
}

/// Issue #973: Result object for paginated metadata search
#[contracttype]
#[derive(Clone)]
pub struct MetadataSearchResult {
    pub ip_ids: soroban_sdk::Vec<u64>,
    pub total_count: u32,
    pub offset: u32,
    pub limit: u32,
    /// Cursor for next page (if any)
    pub next_cursor: Option<BytesN<32>>,
}

/// #974: Merkle tree proof for batch commitments
#[contracttype]
#[derive(Clone)]
pub struct MerkleProof {
    pub leaf_index: u32,
    pub proof_path: soroban_sdk::Vec<BytesN<32>>,
}

/// #974: Merkle root record for tracking batch commitments
#[contracttype]
#[derive(Clone)]
pub struct MerkleRootRecord {
    pub root_id: u64,
    pub root_hash: BytesN<32>,
    pub owner: Address,
    pub timestamp: u64,
    pub leaf_count: u32,
}

/// #975: Time-lock record for time-locked commitments
#[contracttype]
#[derive(Clone)]
pub struct TimeLockRecord {
    pub commitment_id: u64,
    pub unlock_time: u64,
    pub revealed: bool,
}

/// #976: Amendment record for tracking commitment amendments
#[contracttype]
#[derive(Clone)]
pub struct CommitmentAmendment {
    pub amendment_id: u64,
    pub commitment_id: u64,
    pub old_hash: BytesN<32>,
    pub new_hash: BytesN<32>,
    pub timestamp: u64,
    pub amender: Address,
}

/// #977: Privacy level enumeration for access control
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PrivacyLevel {
    Public = 0,
    Private = 1,
    Restricted = 2,
    Confidential = 3,
}

/// #977: Commitment privacy metadata
#[contracttype]
#[derive(Clone)]
pub struct CommitmentPrivacy {
    pub commitment_id: u64,
    pub privacy_level: u32,
    pub allowed_addresses: soroban_sdk::Vec<Address>,
}
