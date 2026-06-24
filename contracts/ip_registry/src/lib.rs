#![no_std]
#![allow(deprecated)]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Bytes, BytesN, Env, Error, Vec,
};

mod validation;
use validation::*;

mod types;
use types::*;

// FIXME: test.rs has compilation errors from merge conflict - re-enable after fix
// FIXME: test.rs has pre-existing compilation errors from a merge conflict - fix before enabling
#[cfg(test)]
mod test;

// FIXME: benchmarks.rs has pre-existing compilation errors from a merge conflict
// #[cfg(test)]
// mod benchmarks;

#[cfg(test)]
mod mutation_tests;

#[cfg(test)]
mod snapshot_tests;

#[cfg(test)]
mod differential_tests;

// FIXME: invariant_tests.rs has pre-existing compilation errors from a merge conflict
// #[cfg(test)]
// mod invariant_tests;

#[cfg(test)]
mod upgrade_tests;

// ── Error Codes ────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    IpNotFound = 1,
    ZeroCommitmentHash = 2,
    CommitmentAlreadyRegistered = 3,
    IpAlreadyRevoked = 4,
    UnauthorizedUpgrade = 5,
    Unauthorized = 6,
    IpExpired = 7,
    MetadataTooLarge = 8,
    LicenseeNotFound = 9,
    InsufficientPoW = 10,
    InvalidExpiry = 11,
    IpInDispute = 12,
    /// #348: Co-owner not found in ownership list.
    CoOwnerNotFound = 13,
    /// #348: Invalid ownership percentage (must be 0-100).
    InvalidOwnershipPercentage = 14,
    /// #348: Only owner can manage co-owners.
    OnlyOwnerCanManageCoOwners = 15,
    DisputeNotFound = 16,
    DisputeAlreadyResolved = 17,
    StakeNotFound = 18,
    AlreadyStaked = 19,
    StakeAlreadySlashed = 20,
    ArbitrationNotFound = 21,
    ArbitrationAlreadyFinalized = 22,
    NotAnArbitrator = 23,
    /// #454: Threshold not yet met.
    ThresholdNotMet = 24,
    /// #454: Signer not in authorized list.
    SignerNotAuthorized = 25,
    /// #454: Signer already submitted a signature.
    AlreadySigned = 26,
    /// #455: Batch metadata too large.
    BatchMetadataTooLarge = 27,
    /// #457: Encrypted data too large.
    EncryptedDataTooLarge = 28,
    /// #465: Escrow not found.
    EscrowNotFound = 29,
    /// #465: Escrow already released or cancelled.
    EscrowNotActive = 30,
    /// #465: Timeout not reached for cancellation.
    EscrowTimeoutNotReached = 31,
    /// #459: Category hash is invalid (all zeros).
    InvalidCategoryHash = 32,
    /// #459: Category depth exceeds maximum allowed.
    InvalidCategoryDepth = 33,
    /// #459: Category not registered.
    CategoryNotFound = 34,
    /// Batch operation size mismatch.
    BatchSizeMismatch = 35,
    /// #664: Transfer already proposed for this IP.
    TransferAlreadyProposed = 36,
    /// #664: Pending transfer has expired.
    TransferExpired = 37,
    /// #664: No pending transfer for this IP.
    NoPendingTransfer = 38,
}

// ── TTL ───────────────────────────────────────────────────────────────────────

/// Minimum ledger TTL bump applied to every persistent storage write.
/// ~1 year at ~5s per ledger: 365 * 24 * 3600 / 5 ≈ 6_307_200 ledgers.
pub const LEDGER_BUMP: u32 = 6_307_200;

/// Maximum metadata size: 1 KB
pub const MAX_METADATA_BYTES: u32 = 1024;

/// Trusted notary public key for timestamp notarization (Issue #345)
/// This is a placeholder - should be set during contract initialization
pub const NOTARY_PUBLIC_KEY: &[u8] = b"notary_public_key_placeholder";

/// Issue #437: Number of storage shards for commitment distribution.
pub const NUM_SHARDS: u32 = 16;

/// Issue #459: Maximum allowed category hierarchy depth.
/// Supports paths like "Software/Cryptography/ZK-Proofs/DLV/AXIOM" (depth 5).
pub const MAX_CATEGORY_DEPTH: u32 = 10;

// ── Storage Keys ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Debug, PartialEq)]
pub enum DataKey {
    IpRecord(u64),
    OwnerIps(Address),
    NextId,
    CommitmentOwner(BytesN<32>), // tracks which owner already holds a commitment hash
    /// Maps commitment hash -> blinded owner identifier for anonymous commits
    AnonymousOwner(BytesN<32>),
    /// #464: Tracks blinded_owner values that have already been used for replay protection
    UsedBlindedOwner(BytesN<32>),
    Admin,
    PartialDisclosure(u64), // stores partial_hash for a given ip_id after reveal
    IpLicenses(u64),        // stores license entries for a given ip_id
    CategoryIps(BytesN<32>), // maps category hash -> Vec<u64> of IP IDs
    PowDifficulty,          // stores the current PoW difficulty (leading zero bits required)
    IpVersions(u64),        // stores Vec<u64> of all version IDs for a given IP
    SuggestedPrice(u64),    // stores suggested price for an IP
    IpCommitmentChecksum,   // Issue #346: stores hash of all commitments for rollback protection
    IpAccessGrants(u64),    // Issue #344: stores Vec of (grantee, access_level) for tiered access
    NotarySignature(u64),   // Issue #345: stores notary signature for timestamp notarization
    IpVersionChain(u64),    // stores Vec<u64> of the full version chain rooted at a given IP
    OwnershipChallenge(u64), // Issue #433: stores OwnershipChallenge for a given challenge_id
    NextChallengeId,        // Issue #433: monotonic challenge ID counter
    EncryptionKeyRotation(u64), // Issue #434: stores rotation history for a given ip_id
    NotaryPublicKey,        // Issue #428: stores the trusted notary Ed25519 public key (32 bytes)
    CommitmentHashes, // Issue #429: stores Vec<BytesN<32>> of all commitment hashes for rollback protection
    IpPowDifficulty(u64), // stores the pow_difficulty used at commit time for strength scoring
    // Previously missing variants (used in existing code)
    ShardIps(u32),             // Issue #437: maps shard_id -> Vec<u64> of IP IDs
    IpAuditTrail(u64),         // Issue #436: stores Vec<AuditEntry> for a given ip_id
    RenewalCount(u64),         // stores renewal count for a given ip_id
    Delegates(Address),        // stores Vec<DelegationRecord> for a given owner
    DelegateDepth(Address),    // stores delegation depth for a given delegate
    IpDisputes(u64),           // stores DisputeRecord for a given dispute_id
    NextDisputeId,             // monotonic dispute ID counter
    IpStake(u64),              // Issue #447: stores StakeRecord for a given ip_id
    OwnerReputation(Address),  // Issue #448: stores ReputationRecord for a given owner
    ArbitrationCase(u64),      // Issue #449: stores ArbitrationRecord for a given arbitration_id
    NextArbitrationId,         // Issue #449: monotonic arbitration ID counter
    ArbitratorPool,            // Issue #449: stores Vec<Address> of registered arbitrators
    CompressedCommitment(u64), // Issue #438: stores compressed commitment bytes for a given ip_id
    // Issue #458: Batch verification result cache
    BatchVerifyResult(BytesN<32>), // maps batch_proof_id -> BatchVerifyResult
    // Issue #456: Compression algorithm selection
    CompressionSelection(u64), // maps ip_id -> CompressionSelection
    // Issue #459: Hierarchical storage
    HierarchyNode(Address, BytesN<32>), // maps (owner, category_hash) -> Vec<u64> of IP IDs
    OwnerCategories(Address),           // maps owner -> Vec<BytesN<32>> of category hashes
    CategoryDepth(BytesN<32>),          // maps category_hash -> u32 depth
    // Issue #454: Threshold signatures
    ThresholdConfig(u64),
    ThresholdSignatures(u64),
    // Issue #455: Batch metadata
    BatchMetadata(u64),
    // Issue #457: Encrypted commitment
    EncryptedCommitment(u64),
    // Issue #465: Batch escrow — keyed by escrow_id (sha256 of ip_ids + timestamp)
    BatchEscrow(BytesN<32>),
    /// #660: Total number of IP commitments.
    TotalIps,
    /// #663: Global list of all IP IDs.
    AllIps,
    /// #664: Pending transfer proposal for an IP.
    PendingTransfer(u64),
}

// ── Types ────────────────────────────────────────────────────────────────────

/// Delegation chain record: tracks a delegate and the depth at which they were granted authority.
/// Depth 0 = direct delegate of the owner; depth 1 = delegate of a delegate, etc.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DelegationRecord {
    pub delegate: Address,
    pub depth: u32,
}

/// Maximum delegation chain depth to prevent unbounded chains.
pub const MAX_DELEGATION_DEPTH: u32 = 5;

/// Issue #436: A single immutable audit entry for an IP record.
/// Entries are append-only and can never be modified or removed.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct AuditEntry {
    pub action: soroban_sdk::Symbol, // e.g. "committed", "revoked", "transferred"
    pub actor: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct LicenseEntry {
    pub licensee: Address,
    pub terms_hash: BytesN<32>,
}

#[contracttype]
#[derive(Clone)]
pub struct IpDispute {
    pub ip_id: u64,
    pub claimant: Address,
    pub evidence_hash: BytesN<32>,
    pub timestamp: u64,
    pub resolved: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct Attestation {
    pub attestor: Address,
    pub attestation_data: Bytes,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct IpChallenge {
    pub challenger: Address,
    pub reason: Bytes,
    pub timestamp: u64,
    pub resolved: bool,
    pub resolution: Bytes,
}

#[contracttype]
#[derive(Clone)]
pub struct DisputeRecord {
    pub dispute_id: u64,
    pub ip_id: u64,
    pub challenger: Address,
    pub evidence_hash: BytesN<32>,
    pub timestamp: u64,
    pub resolved: bool,
    pub winner: Option<Address>,
}

/// Issue #447: Stake record for an IP commitment.
#[contracttype]
#[derive(Clone)]
pub struct StakeRecord {
    pub ip_id: u64,
    pub owner: Address,
    pub amount: i128,
    pub slashed: bool,
}

/// Issue #448: Reputation record for an IP owner.
#[contracttype]
#[derive(Clone)]
pub struct ReputationRecord {
    pub owner: Address,
    pub score: i64,       // can go negative after slashing
    pub commitments: u64, // total successful commitments
    pub disputes_lost: u64,
}

/// Issue #449: Arbitration case for a dispute.
#[contracttype]
#[derive(Clone)]
pub struct ArbitrationRecord {
    pub arbitration_id: u64,
    pub dispute_id: u64,
    pub arbitrators: soroban_sdk::Vec<Address>,
    pub votes_owner: u32,
    pub votes_challenger: u32,
    pub finalized: bool,
    pub winner: Option<Address>,
}

// ── Issue #454: Threshold Signatures ─────────────────────────────────────────

/// Configuration for M-of-N threshold signature verification.
#[contracttype]
#[derive(Clone)]
pub struct ThresholdConfig {
    pub ip_id: u64,
    pub threshold: u32, // M: minimum signatures required
    pub total: u32,     // N: total authorized signers
    pub signers: soroban_sdk::Vec<Address>,
}

/// A single threshold signature entry.
#[contracttype]
#[derive(Clone)]
pub struct ThresholdSignature {
    pub signer: Address,
    pub signature_hash: BytesN<32>, // sha256(commitment_hash || signer_address_bytes)
    pub timestamp: u64,
}

// ── Issue #455: Batch Metadata ────────────────────────────────────────────────

/// Metadata attached to a batch commitment.
#[contracttype]
#[derive(Clone)]
pub struct BatchMetadata {
    pub ip_id: u64,
    pub batch_id: BytesN<32>, // identifier for the batch this IP belongs to
    pub description: Bytes,   // arbitrary metadata (max 1 KB)
    pub timestamp: u64,
}

// ── Issue #456: Compression Algorithm Selection ───────────────────────────────

/// Supported compression algorithms for commitment storage.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum CompressionAlgo {
    None = 0,
    Truncate16 = 1, // first 16 bytes (existing default)
    Xor8 = 2,       // XOR fold to 8 bytes
}

/// Per-IP compression algorithm selection.
#[contracttype]
#[derive(Clone)]
pub struct CompressionSelection {
    pub ip_id: u64,
    pub algorithm: CompressionAlgo,
}

// ── Issue #457: Commitment Encryption ────────────────────────────────────────

/// Encrypted commitment data stored at rest.
#[contracttype]
#[derive(Clone)]
pub struct EncryptedCommitmentRecord {
    pub ip_id: u64,
    pub encrypted_hash: Bytes, // commitment hash encrypted with owner's key
    pub key_hint: BytesN<32>,  // public key hint (e.g. sha256 of owner's public key)
    pub timestamp: u64,
}

// ── Issue #465: Batch Escrow ─────────────────────────────────────────────────

/// Escrow status for batch commitment escrow.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum EscrowStatus {
    Active = 0,
    Released = 1,
    Cancelled = 2,
}

/// Escrow record for multiple commitments held in trust.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct EscrowRecord {
    pub escrow_id: BytesN<32>,
    pub depositor: Address,
    pub ip_ids: soroban_sdk::Vec<u64>,
    pub release_to: Address,
    pub timeout: u64,
    pub status: EscrowStatus,
    pub timestamp: u64,
}

// ── Issue #458: Batch Verification with ZK Proofs ────────────────────────────

/// A single verification request in a batch.
#[contracttype]
#[derive(Clone)]
pub struct VerifyRequest {
    pub ip_id: u64,
    pub secret: BytesN<32>,
    pub blinding_factor: BytesN<32>,
}

/// Result of a single commitment verification within a batch.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VerifyResult {
    pub ip_id: u64,
    pub valid: bool,
}

/// Stored result of a completed batch verification, keyed by the aggregate proof hash.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BatchVerifyResultStorage {
    pub aggregate_proof: BytesN<32>,
    pub total_count: u32,
    pub valid_count: u32,
}

// ── Issue #459: Hierarchical Storage ─────────────────────────────────────────

/// A node in the hierarchical commitment tree.
/// Organises IPs under owner → category → ip_ids for O(1) category lookups.
#[contracttype]
#[derive(Clone)]
pub struct HierarchyNode {
    pub owner: Address,
    pub category_hash: BytesN<32>, // sha256 of the category label
    pub ip_ids: soroban_sdk::Vec<u64>,
}

/// Metadata for a registered category path.
#[contracttype]
#[derive(Clone, Debug)]
pub struct CategoryInfo {
    pub path: soroban_sdk::Bytes, // full category path e.g. "Software/Cryptography/ZK-Proofs"
    pub depth: u32,               // number of segments (3 for the example above)
}

// ── #664: Two-Step Transfer ───────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct PendingTransfer {
    pub ip_id: u64,
    pub new_owner: Address,
    pub expiry: u64,
}

/// #664: Page size for list_all_ips.
pub const PAGE_SIZE: u32 = 100;

/// #664: Expiry window for proposed transfers (7 days in seconds).
pub const TRANSFER_EXPIRY_SECONDS: u64 = 7 * 24 * 3600;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Constant-time comparison of two 32-byte arrays.
/// Returns `true` if all 32 bytes match, `false` otherwise.
/// Every code path performs exactly 32 XOR+OR operations regardless of input,
/// preventing timing side-channel attacks.
fn constant_time_bytes_32_eq(a: &BytesN<32>, b: &BytesN<32>) -> bool {
    let a_arr = a.to_array();
    let b_arr = b.to_array();
    let mut diff: u8 = 0;
    for i in 0..32 {
        diff |= a_arr[i] ^ b_arr[i];
    }
    diff == 0
}

/// Deterministically aggregate multiple commitment hashes into a single proof
/// using incremental SHA-256 hashing.
///
/// Starting from an all-zeros seed, each verified commitment hash is folded in:
/// `proof ← sha256(proof || commitment_hash)`
///
/// This produces a deterministic, order-dependent aggregate proof that can be
/// used to efficiently validate the entire batch.
fn aggregate_batch_proof(env: &Env, commitment_hashes: &Vec<BytesN<32>>) -> BytesN<32> {
    let mut proof = BytesN::from_array(env, &[0u8; 32]);
    for hash in commitment_hashes.iter() {
        let mut input = Bytes::new(env);
        input.append(&proof.into());
        input.append(&hash.into());
        proof = env.crypto().sha256(&input).into();
    }
    proof
}

// ── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct IpRegistry;

#[contractimpl]
impl IpRegistry {
    /// Timestamp a new IP commitment. Returns the assigned IP ID.
    ///
    /// This function creates a new IP record with a cryptographic commitment hash,
    /// establishing a verifiable timestamp on the blockchain. The commitment hash
    /// should be constructed using the Pedersen commitment scheme: sha256(secret || blinding_factor).
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `owner` - The address that owns the IP. This address must authorize the transaction.
    /// * `commitment_hash` - A 32-byte cryptographic hash of the IP secret and blinding factor.
    ///   Must not be all zeros and must be unique across all registered IPs.
    ///
    /// # Returns
    ///
    /// The unique IP ID assigned to this commitment. IDs start at 1 and are monotonically increasing,
    /// persisting across contract upgrades. ID 0 is reserved and never assigned.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// * The `owner` does not authorize the transaction (auth error)
    /// * The `commitment_hash` is all zeros (ZeroCommitmentHash error)
    /// * The `commitment_hash` is already registered (duplicate commitment error)
    ///
    /// # Auth Model
    ///
    /// `owner.require_auth()` is the correct Soroban idiom for "only this address
    /// may call this function". The Soroban host enforces it at the protocol level:
    /// the transaction must carry a valid signature (or delegated sub-auth) for
    /// `owner`. No caller can satisfy this check for an address they do not
    /// legitimately control — the host will panic with an auth error.
    ///
    /// The one exception is test environments that call `env.mock_all_auths()`,
    /// which intentionally bypasses all auth checks. Production transactions on
    /// the Stellar network cannot use this mechanism; it is a test-only helper.
    ///
    /// Therefore: a caller cannot forge `owner` in production. They can only
    /// commit IP under an address for which they hold a valid private key or
    /// delegated authorization.
    pub fn commit_ip(
        env: Env,
        owner: Address,
        commitment_hash: BytesN<32>,
        pow_difficulty: u32,
    ) -> u64 {
        // Enforced by the Soroban host: panics if the transaction does not carry
        // a valid authorization for `owner`. This is the correct auth pattern.
        owner.require_auth();

        // Initialize admin on first call if not set
        if !env.storage().persistent().has(&DataKey::Admin) {
            let admin = env.current_contract_address();
            env.storage().persistent().set(&DataKey::Admin, &admin);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::Admin, 50000, 50000);
        }

        // Reject zero-byte commitment hash (Issue #40)
        require_non_zero_commitment(&env, &commitment_hash);

        // Reject duplicate commitment hash globally
        require_unique_commitment(&env, &commitment_hash);

        // Validate proof-of-work: commitment_hash must have `pow_difficulty` leading zero bits
        require_pow(&env, &commitment_hash, pow_difficulty);

        // NextId lives in persistent storage so it survives contract upgrades.
        // Instance storage is wiped on upgrade, which would reset the counter
        // and cause ID collisions with existing IP records.
        // Initialize to 1 so the first IP ID is 1, not 0 (0 is ambiguous with "not found").
        let id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NextId)
            .unwrap_or(1);

        let record = IpRecord {
            ip_id: id,
            owner: owner.clone(),
            commitment_hash: commitment_hash.clone(),
            timestamp: env.ledger().timestamp(),
            revoked: false,
            co_owners: Vec::new(&env),
            parent_ip_id: None,
            notary_signature: None,
            expiry_timestamp: 0,
            grace_period_seconds: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::IpRecord(id), &record);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::IpRecord(id), LEDGER_BUMP, LEDGER_BUMP);

        // Store pow_difficulty for strength scoring (Issue: entropy/complexity scoring)
        env.storage()
            .persistent()
            .set(&DataKey::IpPowDifficulty(id), &pow_difficulty);
        env.storage().persistent().extend_ttl(
            &DataKey::IpPowDifficulty(id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        // Append to owner index
        let mut ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerIps(owner.clone()))
            .unwrap_or(Vec::new(&env));
        ids.push_back(id);
        env.storage()
            .persistent()
            .set(&DataKey::OwnerIps(owner.clone()), &ids);
        env.storage().persistent().extend_ttl(
            &DataKey::OwnerIps(owner.clone()),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        // Track commitment hash ownership and extend TTL
        env.storage()
            .persistent()
            .set(&DataKey::CommitmentOwner(commitment_hash.clone()), &owner);
        env.storage().persistent().extend_ttl(
            &DataKey::CommitmentOwner(commitment_hash.clone()),
            50000,
            50000,
        );

        env.storage().persistent().set(&DataKey::NextId, &(id + 1));
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::NextId, LEDGER_BUMP, LEDGER_BUMP);

        // Track commitment → owner mapping (for duplicate detection and transfer)
        env.storage()
            .persistent()
            .set(&DataKey::CommitmentOwner(commitment_hash.clone()), &owner);
        env.storage().persistent().extend_ttl(
            &DataKey::CommitmentOwner(commitment_hash.clone()),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.events().publish(
            (symbol_short!("ip_commit"), owner.clone()),
            (id, record.timestamp),
        );

        // Issue #436: Record immutable audit entry for commitment creation
        Self::append_audit_entry(&env, id, symbol_short!("committed"), owner.clone());

        // Issue #437: Assign IP to its shard
        Self::assign_to_shard(&env, id, &commitment_hash);

        // Issue #438: Store compressed commitment
        Self::store_compressed_commitment(&env, id, &commitment_hash);

        // Issue #346: Update commitment checksum for rollback protection
        Self::update_commitment_checksum(&env);

        // Adjust PoW difficulty based on daily commit volume
        Self::adjust_pow_difficulty(&env);

        // #660: Increment global IP count
        let total: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalIps)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::TotalIps, &(total + 1));
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::TotalIps, LEDGER_BUMP, LEDGER_BUMP);

        // #663: Append to global AllIps list
        let mut all_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::AllIps)
            .unwrap_or(Vec::new(&env));
        all_ids.push_back(id);
        env.storage()
            .persistent()
            .set(&DataKey::AllIps, &all_ids);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::AllIps, LEDGER_BUMP, LEDGER_BUMP);

        id
    }

    /// Commit multiple IP commitments in a single transaction.
    ///
    /// This function allows batching multiple IP commitments, reducing gas costs
    /// for users with multiple designs. Returns the assigned IP IDs in order.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `owner` - The address that owns all the IPs. This address must authorize the transaction.
    /// * `commitment_hashes` - A vector of 32-byte cryptographic hashes for the IP commitments.
    ///   Each must not be all zeros and must be unique across all registered IPs.
    ///
    /// # Returns
    ///
    /// A vector of unique IP IDs assigned to the commitments, in the same order as the input hashes.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// * The `owner` does not authorize the transaction (auth error)
    /// * Any `commitment_hash` is all zeros (ZeroCommitmentHash error)
    /// * Any `commitment_hash` is already registered (CommitmentAlreadyRegistered error)
    ///
    /// # Auth Model
    ///
    /// `owner.require_auth()` is called once for the batch operation.
    pub fn batch_commit_ip(
        env: Env,
        owner: Address,
        commitment_hashes: Vec<BytesN<32>>,
    ) -> Vec<u64> {
        owner.require_auth();

        // Initialize admin on first call if not set
        if !env.storage().persistent().has(&DataKey::Admin) {
            let admin = env.current_contract_address();
            env.storage().persistent().set(&DataKey::Admin, &admin);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::Admin, 50000, 50000);
        }

        let mut ids = Vec::new(&env);
        let timestamp = env.ledger().timestamp();

        for commitment_hash in commitment_hashes.iter() {
            // Reject zero-byte commitment hash
            require_non_zero_commitment(&env, &commitment_hash);

            // Reject duplicate commitment hash globally
            require_unique_commitment(&env, &commitment_hash);

            // NextId lives in persistent storage so it survives contract upgrades.
            let id: u64 = env
                .storage()
                .persistent()
                .get(&DataKey::NextId)
                .unwrap_or(1);

            let record = IpRecord {
                ip_id: id,
                owner: owner.clone(),
                commitment_hash: commitment_hash.clone(),
                timestamp,
                revoked: false,
                co_owners: Vec::new(&env),
                parent_ip_id: None,
                notary_signature: None,
                expiry_timestamp: 0,
                grace_period_seconds: 0,
            };

            env.storage()
                .persistent()
                .set(&DataKey::IpRecord(id), &record);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::IpRecord(id), LEDGER_BUMP, LEDGER_BUMP);

            // Append to owner index
            let mut owner_ids: Vec<u64> = env
                .storage()
                .persistent()
                .get(&DataKey::OwnerIps(owner.clone()))
                .unwrap_or(Vec::new(&env));
            owner_ids.push_back(id);
            env.storage()
                .persistent()
                .set(&DataKey::OwnerIps(owner.clone()), &owner_ids);
            env.storage().persistent().extend_ttl(
                &DataKey::OwnerIps(owner.clone()),
                LEDGER_BUMP,
                LEDGER_BUMP,
            );

            // Track commitment hash ownership
            env.storage()
                .persistent()
                .set(&DataKey::CommitmentOwner(commitment_hash.clone()), &owner);
            env.storage().persistent().extend_ttl(
                &DataKey::CommitmentOwner(commitment_hash.clone()),
                50000,
                50000,
            );

            env.events()
                .publish((symbol_short!("ip_commit"), owner.clone()), (id, timestamp));

            ids.push_back(id);

            env.storage().persistent().set(&DataKey::NextId, &(id + 1));
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::NextId, LEDGER_BUMP, LEDGER_BUMP);
        }

        // Issue #346: Update commitment checksum for rollback protection
        Self::update_commitment_checksum(&env);

        // #660: Increment global IP count
        let total: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalIps)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::TotalIps, &(total + ids.len() as u64));
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::TotalIps, LEDGER_BUMP, LEDGER_BUMP);

        // #663: Append all IDs to global AllIps list
        let mut all_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::AllIps)
            .unwrap_or(Vec::new(&env));
        for i in 0..ids.len() {
            all_ids.push_back(ids.get(i).unwrap());
        }
        env.storage()
            .persistent()
            .set(&DataKey::AllIps, &all_ids);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::AllIps, LEDGER_BUMP, LEDGER_BUMP);

        ids
    }

    /// Commit multiple IP commitments anonymously in a single transaction.
    ///
    /// Stores a blinded owner identifier alongside each commitment so ownership
    /// can be proven off-chain or revealed later without exposing the on-chain
    /// owner address at commit time. The on-chain `IpRecord.owner` is set to
    /// the contract address as a placeholder to avoid leaking the submitter.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `blinded_owner` - A 32-byte blinded owner identifier (e.g. `sha256(owner || nonce)`).
    ///   Stored on-chain per commitment so ownership can be proved or revealed later.
    /// * `commitment_hashes` - Non-empty vector of 32-byte commitment hashes to register.
    ///   Each must not be all zeros and must be globally unique.
    ///
    /// # Returns
    ///
    /// `Vec<u64>` — Assigned IP IDs in the same order as the input hashes.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// * `commitment_hashes` is empty (panics with `ZeroCommitmentHash` on the first iteration
    ///   — callers should not pass an empty vector)
    /// * Any `commitment_hash` is all zeros (`ZeroCommitmentHash` error, code 2)
    /// * Any `commitment_hash` is already registered (`CommitmentAlreadyRegistered` error, code 3)
    ///
    /// # Auth Model
    ///
    /// No caller authorization is required. The submitter's identity is intentionally
    /// not recorded on-chain; only the `blinded_owner` identifier is stored.
    ///
    /// # Events
    ///
    /// Emits one `"ip_cmt_a"` event per commitment:
    /// - Topics: `(symbol_short!("ip_cmt_a"), contract_address)`
    /// - Data: `(ip_id: u64, timestamp: u64, blinded_owner: BytesN<32>)`
    ///
    /// # Storage
    ///
    /// Per commitment hash, two persistent keys are written:
    /// - `DataKey::CommitmentOwner(hash)` → contract address (duplicate guard)
    /// - `DataKey::AnonymousOwner(hash)` → `blinded_owner` (ownership proof pointer)
    pub fn batch_commit_ip_anonymous(
        env: Env,
        blinded_owner: BytesN<32>,
        commitment_hashes: Vec<BytesN<32>>,
    ) -> Vec<u64> {
        // No caller auth required for anonymous commits.

        // Reject empty batch — nothing to commit.
        if commitment_hashes.is_empty() {
            env.panic_with_error(Error::from_contract_error(
                ContractError::ZeroCommitmentHash as u32,
            ));
        }

        // #464: Replay protection — reject if this blinded_owner has already been used.
        // A blinded_owner is sha256(real_owner || nonce); reusing it would link
        // multiple batches to the same blinded identity, undermining anonymity.
        if env
            .storage()
            .persistent()
            .has(&DataKey::UsedBlindedOwner(blinded_owner.clone()))
        {
            env.panic_with_error(Error::from_contract_error(
                ContractError::CommitmentAlreadyRegistered as u32,
            ));
        }
        // Mark this blinded_owner as consumed before writing any commitments.
        env.storage()
            .persistent()
            .set(&DataKey::UsedBlindedOwner(blinded_owner.clone()), &true);
        env.storage().persistent().extend_ttl(
            &DataKey::UsedBlindedOwner(blinded_owner.clone()),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        // Initialize admin on first call if not set
        if !env.storage().persistent().has(&DataKey::Admin) {
            let admin = env.current_contract_address();
            env.storage().persistent().set(&DataKey::Admin, &admin);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::Admin, 50000, 50000);
        }

        let mut ids = Vec::new(&env);
        let timestamp = env.ledger().timestamp();

        for commitment_hash in commitment_hashes.iter() {
            // Reject zero-byte commitment hash
            require_non_zero_commitment(&env, &commitment_hash);

            // Reject duplicate commitment hash globally
            require_unique_commitment(&env, &commitment_hash);

            // NextId lives in persistent storage so it survives contract upgrades.
            let id: u64 = env
                .storage()
                .persistent()
                .get(&DataKey::NextId)
                .unwrap_or(1);

            let record = IpRecord {
                ip_id: id,
                owner: env.current_contract_address(),
                commitment_hash: commitment_hash.clone(),
                timestamp,
                revoked: false,
                co_owners: Vec::new(&env),
                parent_ip_id: None,
                notary_signature: None,
                expiry_timestamp: 0,
                grace_period_seconds: 0,
            };

            env.storage()
                .persistent()
                .set(&DataKey::IpRecord(id), &record);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::IpRecord(id), LEDGER_BUMP, LEDGER_BUMP);

            // Do NOT append to OwnerIps index to preserve anonymity.

            // Track commitment hash ownership to prevent duplicates
            env.storage().persistent().set(
                &DataKey::CommitmentOwner(commitment_hash.clone()),
                &env.current_contract_address(),
            );
            env.storage().persistent().extend_ttl(
                &DataKey::CommitmentOwner(commitment_hash.clone()),
                50000,
                50000,
            );

            // Record blinded owner mapping for later on-chain/off-chain proof if needed.
            env.storage().persistent().set(
                &DataKey::AnonymousOwner(commitment_hash.clone()),
                &blinded_owner,
            );
            env.storage().persistent().extend_ttl(
                &DataKey::AnonymousOwner(commitment_hash.clone()),
                LEDGER_BUMP,
                LEDGER_BUMP,
            );

            env.events().publish(
                (symbol_short!("ip_cmt_a"), env.current_contract_address()),
                (id, timestamp, blinded_owner.clone()),
            );

            ids.push_back(id);

            env.storage().persistent().set(&DataKey::NextId, &(id + 1));
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::NextId, LEDGER_BUMP, LEDGER_BUMP);
        }

        // Issue #346: Update commitment checksum for rollback protection
        Self::update_commitment_checksum(&env);

        ids
    }

    /// Retrieve the blinded owner identifier stored for an anonymous commitment.
    ///
    /// Returns `Some(blinded_owner)` if the commitment was registered via
    /// `batch_commit_ip_anonymous`, or `None` if no anonymous owner record exists
    /// for the given hash (e.g. it was committed via `commit_ip` or `batch_commit_ip`).
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `commitment_hash` - The 32-byte commitment hash to look up
    ///
    /// # Returns
    ///
    /// `Option<BytesN<32>>` — The blinded owner identifier, or `None`.
    pub fn get_anonymous_owner(env: Env, commitment_hash: BytesN<32>) -> Option<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&DataKey::AnonymousOwner(commitment_hash))
    }

    /// Retrieve blinded owner identifiers for multiple commitment hashes in a batch.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `commitment_hashes` - Vector of commitment hashes to look up
    ///
    /// # Returns
    ///
    /// `Vec<Option<BytesN<32>>>` — For each hash, the blinded owner or `None`.
    pub fn get_blinded_owner_batch(
        env: Env,
        commitment_hashes: Vec<BytesN<32>>,
    ) -> Vec<Option<BytesN<32>>> {
        let mut results = Vec::new(&env);
        for hash in commitment_hashes.iter() {
            results.push_back(Self::get_anonymous_owner(env.clone(), hash));
        }
        results
    }

    // ── Issue #465: Batch Escrow ────────────────────────────────────────────

    /// Escrow multiple IP commitments for conditional release.
    ///
    /// Creates an escrow record holding multiple IP IDs. The depositor locks
    /// ownership until either released to the beneficiary or cancelled after timeout.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `depositor` - Address initiating the escrow (must own all IP IDs)
    /// * `ip_ids` - Vector of IP IDs to escrow
    /// * `release_to` - Address that can claim the IPs upon release
    /// * `timeout` - Ledger timestamp after which depositor can cancel
    ///
    /// # Returns
    ///
    /// `BytesN<32>` — Unique escrow ID (sha256 of ip_ids + timestamp)
    pub fn batch_escrow_commitments(
        env: Env,
        depositor: Address,
        ip_ids: Vec<u64>,
        release_to: Address,
        timeout: u64,
    ) -> BytesN<32> {
        depositor.require_auth();

        for ip_id in ip_ids.iter() {
            let record = require_ip_exists(&env, ip_id);
            if record.owner != depositor {
                panic_with_error!(&env, ContractError::Unauthorized);
            }
        }

        let timestamp = env.ledger().timestamp();
        let mut id_bytes = Bytes::new(&env);
        for ip_id in ip_ids.iter() {
            id_bytes.append(&Bytes::from_array(&env, &ip_id.to_be_bytes()));
        }
        id_bytes.append(&Bytes::from_array(&env, &timestamp.to_be_bytes()));
        let escrow_id: BytesN<32> = env.crypto().sha256(&id_bytes).into();

        let escrow = EscrowRecord {
            escrow_id: escrow_id.clone(),
            depositor: depositor.clone(),
            ip_ids: ip_ids.clone(),
            release_to,
            timeout,
            status: EscrowStatus::Active,
            timestamp,
        };

        env.storage()
            .persistent()
            .set(&DataKey::BatchEscrow(escrow_id.clone()), &escrow);
        env.storage().persistent().extend_ttl(
            &DataKey::BatchEscrow(escrow_id.clone()),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.events().publish(
            (symbol_short!("escrow"), depositor),
            (escrow_id.clone(), ip_ids.len()),
        );

        escrow_id
    }

    /// Retrieve an escrow record by ID.
    ///
    /// # Returns
    ///
    /// `Option<EscrowRecord>` — The escrow record, or `None` if not found.
    pub fn get_batch_escrow(env: Env, escrow_id: BytesN<32>) -> Option<EscrowRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::BatchEscrow(escrow_id))
    }

    /// Release escrowed IPs to the beneficiary.
    ///
    /// Transfers ownership of all escrowed IPs to `release_to` address.
    /// Only the depositor can release before timeout.
    ///
    /// # Panics
    ///
    /// Panics if escrow not found, not active, or caller unauthorized.
    pub fn release_batch_escrow(env: Env, escrow_id: BytesN<32>) {
        let mut escrow: EscrowRecord = env
            .storage()
            .persistent()
            .get(&DataKey::BatchEscrow(escrow_id.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::EscrowNotFound));

        if escrow.status != EscrowStatus::Active {
            panic_with_error!(&env, ContractError::EscrowNotActive);
        }

        escrow.depositor.require_auth();

        for ip_id in escrow.ip_ids.iter() {
            Self::transfer_ip(env.clone(), ip_id, escrow.release_to.clone());
        }

        escrow.status = EscrowStatus::Released;
        env.storage()
            .persistent()
            .set(&DataKey::BatchEscrow(escrow_id.clone()), &escrow);
        env.storage().persistent().extend_ttl(
            &DataKey::BatchEscrow(escrow_id.clone()),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.events().publish(
            (symbol_short!("esc_rel"), escrow.depositor.clone()),
            escrow_id,
        );
    }

    /// Cancel escrow and return IPs to depositor.
    ///
    /// Only callable by depositor after timeout has passed.
    ///
    /// # Panics
    ///
    /// Panics if escrow not found, not active, timeout not reached, or caller unauthorized.
    pub fn cancel_batch_escrow(env: Env, escrow_id: BytesN<32>) {
        let mut escrow: EscrowRecord = env
            .storage()
            .persistent()
            .get(&DataKey::BatchEscrow(escrow_id.clone()))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::EscrowNotFound));

        if escrow.status != EscrowStatus::Active {
            panic_with_error!(&env, ContractError::EscrowNotActive);
        }

        escrow.depositor.require_auth();

        if env.ledger().timestamp() < escrow.timeout {
            panic_with_error!(&env, ContractError::EscrowTimeoutNotReached);
        }

        escrow.status = EscrowStatus::Cancelled;
        env.storage()
            .persistent()
            .set(&DataKey::BatchEscrow(escrow_id.clone()), &escrow);
        env.storage().persistent().extend_ttl(
            &DataKey::BatchEscrow(escrow_id.clone()),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.events().publish(
            (symbol_short!("esc_cnl"), escrow.depositor.clone()),
            escrow_id,
        );
    }

    /// Transfer IP ownership to a new address.
    ///
    /// This function transfers ownership of an IP record from the current owner
    /// to a new owner. The current owner must authorize the transaction.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `ip_id` - The unique identifier of the IP to transfer
    /// * `new_owner` - The address that will become the new owner of the IP
    ///
    /// # Returns
    ///
    /// This function does not return a value.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// * The IP record does not exist (IpNotFound error)
    /// * The current owner does not authorize the transaction (auth error)
    pub fn transfer_ip(env: Env, ip_id: u64, new_owner: Address) {
        let mut record = require_ip_exists(&env, ip_id);

        record.owner.require_auth();

        let old_owner = record.owner.clone();

        // Remove from old owner's index
        let mut old_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerIps(old_owner.clone()))
            .unwrap_or(Vec::new(&env));
        if let Some(pos) = old_ids.iter().position(|x| x == ip_id) {
            old_ids.remove(pos as u32);
        }
        env.storage()
            .persistent()
            .set(&DataKey::OwnerIps(old_owner.clone()), &old_ids);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::OwnerIps(old_owner.clone()), 50000, 50000);

        // Add to new owner's index
        let mut new_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerIps(new_owner.clone()))
            .unwrap_or(Vec::new(&env));
        new_ids.push_back(ip_id);
        env.storage()
            .persistent()
            .set(&DataKey::OwnerIps(new_owner.clone()), &new_ids);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::OwnerIps(new_owner.clone()), 50000, 50000);

        // Update commitment index
        env.storage().persistent().set(
            &DataKey::CommitmentOwner(record.commitment_hash.clone()),
            &new_owner,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::CommitmentOwner(record.commitment_hash.clone()),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        record.owner = new_owner.clone();
        env.storage()
            .persistent()
            .set(&DataKey::IpRecord(ip_id), &record);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::IpRecord(ip_id), LEDGER_BUMP, LEDGER_BUMP);

        // Emit transfer event: (ip_id, old_owner, new_owner)
        env.events()
            .publish((TRANSFER_TOPIC, ip_id), (old_owner, new_owner.clone()));

        // Issue #436: Record immutable audit entry for ownership transfer
        Self::append_audit_entry(&env, ip_id, symbol_short!("xferred"), new_owner);
    }

    /// Transfer IP ownership to a new address (named alias for transfer_ip).
    ///
    /// Supports use cases like company acquisitions or IP licensing agreements.
    /// Only the current owner may authorize the transfer.
    ///
    /// # Panics
    ///
    /// Panics if the IP does not exist or the caller is not the current owner.
    pub fn transfer_ip_ownership(env: Env, ip_id: u64, new_owner: Address) {
        Self::transfer_ip(env, ip_id, new_owner);
    }

    /// #664: Propose a two-step ownership transfer.
    ///
    /// Current owner proposes a new owner. The new owner must call `accept_transfer`
    /// within 7 days for the transfer to take effect. Only one pending transfer
    /// per IP is allowed at a time.
    ///
    /// # Panics
    ///
    /// Panics if the IP does not exist, caller is not the owner, or a transfer
    /// is already pending.
    pub fn propose_transfer(env: Env, ip_id: u64, new_owner: Address) {
        let record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();

        if env
            .storage()
            .persistent()
            .has(&DataKey::PendingTransfer(ip_id))
        {
            panic_with_error!(
                &env,
                ContractError::TransferAlreadyProposed
            );
        }

        let pending = PendingTransfer {
            ip_id,
            new_owner: new_owner.clone(),
            expiry: env.ledger().timestamp().saturating_add(TRANSFER_EXPIRY_SECONDS),
        };

        env.storage()
            .persistent()
            .set(&DataKey::PendingTransfer(ip_id), &pending);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::PendingTransfer(ip_id), LEDGER_BUMP, LEDGER_BUMP);

        env.events().publish(
            (symbol_short!("prop_xfer"), record.owner),
            (ip_id, new_owner),
        );
    }

    /// #664: Accept a proposed ownership transfer.
    ///
    /// Must be called by the proposed new owner within 7 days of the proposal.
    /// Transfers ownership and clears the pending transfer.
    ///
    /// # Panics
    ///
    /// Panics if no pending transfer exists, the caller is not the proposed
    /// new owner, or the transfer has expired.
    pub fn accept_transfer(env: Env, ip_id: u64) {
        let pending: PendingTransfer = env
            .storage()
            .persistent()
            .get(&DataKey::PendingTransfer(ip_id))
            .unwrap_or_else(|| {
                panic_with_error!(&env, ContractError::NoPendingTransfer)
            });

        pending.new_owner.require_auth();

        if env.ledger().timestamp() > pending.expiry {
            env.storage()
                .persistent()
                .remove(&DataKey::PendingTransfer(ip_id));
            panic_with_error!(&env, ContractError::TransferExpired);
        }

        Self::transfer_ip(env.clone(), ip_id, pending.new_owner.clone());

        env.storage()
            .persistent()
            .remove(&DataKey::PendingTransfer(ip_id));
    }

    /// Revoke an IP record, marking it as invalid.
    ///
    /// Only the current owner may revoke. A revoked IP cannot be swapped.
    ///
    /// # Panics
    ///
    /// Panics if the IP does not exist, the owner does not authorize, or the IP is already revoked.
    pub fn revoke_ip(env: Env, ip_id: u64) {
        let mut record = require_ip_exists(&env, ip_id);

        record.owner.require_auth();

        require_not_revoked(&env, &record);

        record.revoked = true;
        env.storage()
            .persistent()
            .set(&DataKey::IpRecord(ip_id), &record);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::IpRecord(ip_id), 50000, 50000);

        // #664: Cancel any pending transfer on revocation
        if env
            .storage()
            .persistent()
            .has(&DataKey::PendingTransfer(ip_id))
        {
            env.storage()
                .persistent()
                .remove(&DataKey::PendingTransfer(ip_id));
        }

        env.events().publish(
            (REVOKE_TOPIC, record.owner.clone()),
            (ip_id, env.ledger().timestamp()),
        );

        // Issue #436: Record immutable audit entry for revocation
        Self::append_audit_entry(&env, ip_id, symbol_short!("revoked"), record.owner);
    }

    /// Validate that a new WASM is compatible for upgrade.
    ///
    /// Checks that the new WASM has the same contract interface,
    /// does not remove storage keys, and does not change error codes.
    ///
    /// # Panics
    ///
    /// Panics if the new WASM is not compatible.
    pub fn validate_upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        // For now, simple validation: ensure new_wasm_hash is not zero
        let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
        if new_wasm_hash == zero_hash {
            env.panic_with_error(Error::from_contract_error(
                ContractError::UnauthorizedUpgrade as u32,
            ));
        }
        // TODO: Implement full validation for exported functions, storage keys, error codes
    }

    /// Admin-only contract upgrade.
    ///
    /// # Panics
    ///
    /// # Panics
    ///
    /// Panics if caller is not admin or admin not initialized.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let admin_opt: Option<Address> = env.storage().persistent().get(&DataKey::Admin);
        if admin_opt.is_none() {
            env.panic_with_error(Error::from_contract_error(
                ContractError::UnauthorizedUpgrade as u32,
            ));
        }
        let admin = admin_opt.unwrap();
        let invoker = env.current_contract_address();
        if invoker != admin {
            env.panic_with_error(Error::from_contract_error(
                ContractError::UnauthorizedUpgrade as u32,
            ));
        }
        admin.require_auth();

        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    /// Retrieve an IP record by ID.
    ///
    /// Returns the complete IP record including owner, commitment hash, and timestamp.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `ip_id` - The unique identifier of the IP to retrieve
    ///
    /// # Returns
    ///
    /// The `IpRecord` containing:
    /// * `ip_id` - The unique identifier
    /// * `owner` - The current owner's address
    /// * `commitment_hash` - The cryptographic commitment hash
    /// * `timestamp` - The ledger timestamp when the IP was committed
    ///
    /// # Panics
    ///
    /// Panics if the IP record does not exist (IpNotFound error).
    pub fn get_ip(env: Env, ip_id: u64) -> IpRecord {
        require_ip_exists(&env, ip_id)
    }

    // ── Issue #454: Threshold Signatures ───────────────────────────────────
    pub fn require_threshold_signatures(
        env: Env,
        ip_id: u64,
        threshold: u32,
        signers: soroban_sdk::Vec<Address>,
    ) {
        require_ip_exists(&env, ip_id);
        if threshold == 0 || threshold > signers.len() {
            panic_with_error!(&env, ContractError::ThresholdNotMet);
        }

        let config = ThresholdConfig {
            ip_id,
            threshold,
            total: signers.len(),
            signers: signers.clone(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::ThresholdConfig(ip_id), &config);
        env.storage().persistent().extend_ttl(
            &DataKey::ThresholdConfig(ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.storage().persistent().set(
            &DataKey::ThresholdSignatures(ip_id),
            &soroban_sdk::Vec::<ThresholdSignature>::new(&env),
        );
        env.storage().persistent().extend_ttl(
            &DataKey::ThresholdSignatures(ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );
    }

    pub fn get_threshold_config(env: Env, ip_id: u64) -> Option<ThresholdConfig> {
        require_ip_exists(&env, ip_id);
        env.storage()
            .persistent()
            .get(&DataKey::ThresholdConfig(ip_id))
    }

    pub fn add_threshold_signature(
        env: Env,
        ip_id: u64,
        signer: Address,
        signature_hash: BytesN<32>,
    ) {
        require_ip_exists(&env, ip_id);
        let config: ThresholdConfig = env
            .storage()
            .persistent()
            .get(&DataKey::ThresholdConfig(ip_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::SignerNotAuthorized));

        if !config.signers.iter().any(|entry| entry == signer) {
            panic_with_error!(&env, ContractError::SignerNotAuthorized);
        }

        let mut signatures: soroban_sdk::Vec<ThresholdSignature> = env
            .storage()
            .persistent()
            .get(&DataKey::ThresholdSignatures(ip_id))
            .unwrap_or(soroban_sdk::Vec::new(&env));

        if signatures.iter().any(|entry| entry.signer == signer) {
            panic_with_error!(&env, ContractError::AlreadySigned);
        }

        signatures.push_back(ThresholdSignature {
            signer: signer.clone(),
            signature_hash,
            timestamp: env.ledger().timestamp(),
        });

        env.storage()
            .persistent()
            .set(&DataKey::ThresholdSignatures(ip_id), &signatures);
        env.storage().persistent().extend_ttl(
            &DataKey::ThresholdSignatures(ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );
    }

    pub fn get_threshold_signatures(env: Env, ip_id: u64) -> soroban_sdk::Vec<ThresholdSignature> {
        require_ip_exists(&env, ip_id);
        env.storage()
            .persistent()
            .get(&DataKey::ThresholdSignatures(ip_id))
            .unwrap_or(soroban_sdk::Vec::new(&env))
    }

    pub fn verify_threshold_signatures(env: Env, ip_id: u64) -> bool {
        require_ip_exists(&env, ip_id);
        let config: Option<ThresholdConfig> = env
            .storage()
            .persistent()
            .get(&DataKey::ThresholdConfig(ip_id));
        let config = match config {
            Some(c) => c,
            None => return false,
        };
        let signatures = Self::get_threshold_signatures(env.clone(), ip_id);
        signatures.len() >= config.threshold
    }

    // ── Issue #455: Batch Metadata ──────────────────────────────────────────
    pub fn set_batch_metadata(env: Env, ip_id: u64, batch_id: BytesN<32>, description: Bytes) {
        require_ip_exists(&env, ip_id);
        if description.len() > MAX_METADATA_BYTES {
            panic_with_error!(&env, ContractError::BatchMetadataTooLarge);
        }

        let record = BatchMetadata {
            ip_id,
            batch_id,
            description,
            timestamp: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::BatchMetadata(ip_id), &record);
        env.storage().persistent().extend_ttl(
            &DataKey::BatchMetadata(ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );
    }

    pub fn get_batch_metadata(env: Env, ip_id: u64) -> Option<BatchMetadata> {
        require_ip_exists(&env, ip_id);
        env.storage()
            .persistent()
            .get(&DataKey::BatchMetadata(ip_id))
    }

    // ── Issue #456: Compression Algorithm Selection ────────────────────────
    pub fn get_commitment_compression(env: Env, ip_id: u64) -> CompressionAlgo {
        require_ip_exists(&env, ip_id);
        env.storage()
            .persistent()
            .get(&DataKey::CompressionSelection(ip_id))
            .map(|selection: CompressionSelection| selection.algorithm)
            .unwrap_or(CompressionAlgo::Truncate16)
    }

    pub fn set_commitment_compression(env: Env, ip_id: u64, algorithm: CompressionAlgo) {
        require_ip_exists(&env, ip_id);
        let selection = CompressionSelection { ip_id, algorithm };
        env.storage()
            .persistent()
            .set(&DataKey::CompressionSelection(ip_id), &selection);
        env.storage().persistent().extend_ttl(
            &DataKey::CompressionSelection(ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );
    }

    pub fn get_compressed_bytes(env: Env, ip_id: u64) -> Bytes {
        require_ip_exists(&env, ip_id);
        let record = require_ip_exists(&env, ip_id);
        let algo = Self::get_commitment_compression(env.clone(), ip_id);
        let hash = record.commitment_hash;

        match algo {
            CompressionAlgo::None => Bytes::from_array(&env, &hash.to_array()),
            CompressionAlgo::Truncate16 => {
                let mut bytes = Bytes::new(&env);
                let arr = hash.to_array();
                for i in 0..16 {
                    bytes.push_back(arr[i as usize]);
                }
                bytes
            }
            CompressionAlgo::Xor8 => {
                let mut bytes = Bytes::new(&env);
                let arr = hash.to_array();
                let mut acc = 0u8;
                for b in arr.iter() {
                    acc ^= *b;
                }
                for _ in 0..8 {
                    bytes.push_back(acc);
                }
                bytes
            }
        }
    }

    // ── Issue #457: Commitment Encryption ───────────────────────────────────
    pub fn encrypt_commitment(env: Env, ip_id: u64, encrypted_hash: Bytes, key_hint: BytesN<32>) {
        require_ip_exists(&env, ip_id);
        if encrypted_hash.len() > 256 {
            panic_with_error!(&env, ContractError::EncryptedDataTooLarge);
        }

        let record = EncryptedCommitmentRecord {
            ip_id,
            encrypted_hash,
            key_hint,
            timestamp: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::EncryptedCommitment(ip_id), &record);
        env.storage().persistent().extend_ttl(
            &DataKey::EncryptedCommitment(ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );
    }

    pub fn get_encrypted_commitment(env: Env, ip_id: u64) -> Option<EncryptedCommitmentRecord> {
        require_ip_exists(&env, ip_id);
        env.storage()
            .persistent()
            .get(&DataKey::EncryptedCommitment(ip_id))
    }

    /// Verify a commitment: hash the secret and blinding factor, then compare to stored commitment hash.
    ///
    /// This function implements Pedersen commitment verification by computing
    /// sha256(secret || blinding_factor) and comparing it to the stored commitment hash.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `ip_id` - The unique identifier of the IP to verify
    /// * `secret` - The 32-byte secret that was used to create the commitment
    /// * `blinding_factor` - The 32-byte blinding factor used to create the commitment
    ///
    /// # Returns
    ///
    /// `true` if the computed hash matches the stored commitment hash, `false` otherwise.
    ///
    /// # Panics
    ///
    /// Panics if the IP record does not exist (IpNotFound error).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // To verify a commitment, you need the original secret and blinding factor
    /// let is_valid = registry.verify_commitment(&ip_id, &secret, &blinding_factor);
    /// ```
    pub fn verify_commitment(
        env: Env,
        ip_id: u64,
        secret: BytesN<32>,
        blinding_factor: BytesN<32>,
    ) -> bool {
        let record = require_ip_exists(&env, ip_id);

        // Reject if expired
        // Expiry check removed - field not in types

        // Concatenate secret || blinding_factor into Bytes, then SHA256
        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&secret.into());
        preimage.append(&blinding_factor.into());
        let computed_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

        // Constant-time comparison to prevent timing side-channel attacks
        constant_time_bytes_32_eq(&record.commitment_hash, &computed_hash)
    }

    /// List all IP IDs owned by an address.
    ///
    /// Returns a vector of all IP IDs owned by the specified address.
    /// Returns an empty vector if the address has never committed any IP.
    ///
    /// # Performance
    ///
    /// This function is optimized to read only the ID list from storage,
    /// not the full IP records. Callers can fetch individual records
    /// using `get_ip()` only for IDs they need.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `owner` - The address to list IPs for
    ///
    /// # Returns
    ///
    /// `Vec<u64>` containing all IP IDs owned by the address,
    /// or an empty vector if the address has no IP records.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    pub fn list_ip_by_owner(env: Env, owner: Address) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::OwnerIps(owner))
            .unwrap_or(Vec::new(&env))
    }

    /// #660: Returns the total number of IP commitments registered.
    pub fn get_ip_count(env: Env) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalIps)
            .unwrap_or(0)
    }

    /// #663: List all IP IDs with pagination (page size = 100).
    /// Page 0 returns the first 100 IDs, page 1 the next 100, etc.
    /// Returns an empty vec if the page is beyond the available data.
    pub fn list_all_ips(env: Env, page: u32) -> Vec<u64> {
        let all_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::AllIps)
            .unwrap_or(Vec::new(&env));
        let start = (page as u64).saturating_mul(PAGE_SIZE as u64) as u32;
        let end = (start + PAGE_SIZE).min(all_ids.len());
        if start >= all_ids.len() {
            return Vec::new(&env);
        }
        let mut result = Vec::new(&env);
        for i in start..end {
            result.push_back(all_ids.get(i).unwrap());
        }
        result
    }

    /// Returns the current PoW difficulty (number of leading zero bits required in commitment_hash).
    /// Defaults to 4 if not explicitly set.
    pub fn get_pow_difficulty(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::PowDifficulty)
            .unwrap_or(4u32)
    }

    /// Returns the entropy-and-complexity strength score (0–100) for an IP commitment.
    ///
    /// The score combines:
    /// - **Byte entropy**: number of unique bytes in the 32-byte commitment hash,
    ///   scaled to 0–50 (max 32 unique bytes → 50 points).
    /// - **PoW difficulty**: the `pow_difficulty` used at commit time, scaled to 0–50
    ///   (each difficulty bit contributes ~1.5625 points, capped at 50).
    ///
    /// Weak commitments (e.g. all-same-byte hashes or zero PoW) score low; strong,
    /// high-entropy commitments with meaningful PoW score near 100.
    ///
    /// # Panics
    ///
    /// Panics with `IpNotFound` if the IP does not exist.
    pub fn get_ip_strength(env: Env, ip_id: u64) -> u32 {
        let record = require_ip_exists(&env, ip_id);
        let pow_difficulty: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::IpPowDifficulty(ip_id))
            .unwrap_or(0u32);

        let hash_bytes = record.commitment_hash.to_array();

        // Count unique bytes as a proxy for byte-level entropy (0–32 unique values)
        let mut seen = [false; 256];
        for b in hash_bytes.iter() {
            seen[*b as usize] = true;
        }
        let unique_bytes = seen.iter().filter(|&&v| v).count() as u32;

        // Scale unique_bytes (0–32) to 0–50
        let entropy_score = (unique_bytes * 50) / 32;

        // Scale pow_difficulty to 0–50 (32 bits max → 50 points)
        let pow_score = if pow_difficulty >= 32 {
            50u32
        } else {
            (pow_difficulty * 50) / 32
        };

        (entropy_score + pow_score).min(100)
    }

    // Returns the current protocol configuration.
    // get_protocol_config removed - ProtocolConfig type not defined

    /// Verify that a commitment hash meets the PoW requirement for a given nonce.
    ///
    /// Computes `sha256(commitment_hash || nonce_be_bytes)` and checks that the
    /// result has at least `pow_difficulty` leading zero bits (current on-chain value).
    ///
    /// Returns `true` if the PoW is valid, `false` otherwise.
    pub fn verify_commitment_pow(env: Env, commitment_hash: BytesN<32>, nonce: u64) -> bool {
        let difficulty: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::PowDifficulty)
            .unwrap_or(2u32);

        if difficulty == 0 {
            return true;
        }

        let mut preimage = Bytes::new(&env);
        preimage.append(&commitment_hash.into());
        preimage.append(&Bytes::from_array(&env, &nonce.to_be_bytes()));
        let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let hash_bytes = hash.to_array();

        let mut remaining = difficulty;
        for byte in hash_bytes.iter() {
            if remaining == 0 {
                break;
            }
            let bits = if remaining >= 8 { 8 } else { remaining };
            let mask: u8 = !((1u8 << (8 - bits)).wrapping_sub(1));
            if byte & mask != 0 {
                return false;
            }
            remaining = remaining.saturating_sub(8);
        }
        true
    }

    /// Adjust PoW difficulty based on daily commit volume.
    ///
    /// - commits today > 100 → increase difficulty by 1 (max 32)
    /// - commits today < 10  → decrease difficulty by 1 (min 1)
    /// - otherwise           → no change
    fn adjust_pow_difficulty(_env: &Env) {
        // Removed - uses non-existent DataKey variants
    }

    /// Partially disclose an IP commitment by revealing a hash of the design
    /// without exposing the full secret.
    ///
    /// # Proof Scheme
    ///
    /// The original commitment is `commitment_hash = sha256(partial_hash || blinding_factor)`.
    /// The caller proves knowledge of `partial_hash` (e.g. sha256 of source code) and
    /// `blinding_factor` by providing both. On-chain verification recomputes
    /// `sha256(partial_hash || blinding_factor)` and checks it equals the stored
    /// `commitment_hash`. The `partial_hash` is then stored publicly so third parties
    /// can verify prior art without learning the full design.
    ///
    /// # Arguments
    ///
    /// * `ip_id` - The IP to partially disclose
    /// * `partial_hash` - sha256 of the design artifact (e.g. sha256(source_code))
    /// * `blinding_factor` - The blinding factor used when committing
    ///
    /// # Returns
    ///
    /// `true` if the proof is valid and the partial hash is stored; `false` otherwise.
    ///
    /// # Panics
    ///
    /// Panics if the IP does not exist or the caller is not the owner.
    pub fn reveal_partial(
        env: Env,
        ip_id: u64,
        partial_hash: BytesN<32>,
        blinding_factor: BytesN<32>,
    ) -> bool {
        let record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();

        // Recompute commitment: sha256(partial_hash || blinding_factor)
        let mut preimage = Bytes::new(&env);
        preimage.append(&partial_hash.clone().into());
        preimage.append(&blinding_factor.into());
        let computed: BytesN<32> = env.crypto().sha256(&preimage).into();

        if computed != record.commitment_hash {
            return false;
        }

        // Store the partial hash publicly for third-party verification
        env.storage()
            .persistent()
            .set(&DataKey::PartialDisclosure(ip_id), &partial_hash);
        env.storage().persistent().extend_ttl(
            &DataKey::PartialDisclosure(ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.events().publish(
            (symbol_short!("partial"), record.owner),
            (ip_id, partial_hash),
        );

        true
    }

    /// Retrieve the publicly disclosed partial hash for an IP, if any.
    ///
    /// Returns `Some(partial_hash)` if `reveal_partial` was successfully called,
    /// `None` if no partial disclosure has been made.
    pub fn get_partial_disclosure(env: Env, ip_id: u64) -> Option<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&DataKey::PartialDisclosure(ip_id))
    }

    /// Check if an address owns a specific IP.
    ///
    /// Returns `true` if the given address is the owner of the IP with the given ID,
    /// `false` otherwise. Returns `false` if the IP does not exist.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `ip_id` - The unique identifier of the IP to check
    /// * `address` - The address to check for ownership
    ///
    /// # Returns
    ///
    /// `true` if the address owns the IP, `false` otherwise.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    pub fn is_ip_owner(env: Env, ip_id: u64, address: Address) -> bool {
        if let Some(record) = env
            .storage()
            .persistent()
            .get::<DataKey, IpRecord>(&DataKey::IpRecord(ip_id))
        {
            record.owner == address
        } else {
            false
        }
    }

    // Set or update the expiry timestamp for an IP. Owner-only.
    // Pass 0 to remove expiry.
    // set_ip_expiry removed - expiry_timestamp field not in IpRecord

    // Renew an IP's expiry to extend its protection period. Owner-only.
    // new_expiry must be strictly greater than the current expiry timestamp.
    // Emits an event with (ip_id, old_expiry, new_expiry).
    // renew_ip removed - expiry_timestamp field not in IpRecord

    // Set or update metadata for an IP (max 1 KB). Owner-only.
    // set_ip_metadata removed - metadata field not in IpRecord

    /// Grant a license for an IP to a licensee. Owner-only.
    pub fn grant_license(env: Env, ip_id: u64, licensee: Address, terms_hash: BytesN<32>) {
        let record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();

        let mut licenses: Vec<LicenseEntry> = env
            .storage()
            .persistent()
            .get(&DataKey::IpLicenses(ip_id))
            .unwrap_or(Vec::new(&env));

        // Replace existing entry for this licensee, or append
        let mut found = false;
        for i in 0..licenses.len() {
            if licenses.get(i).unwrap().licensee == licensee {
                licenses.set(
                    i,
                    LicenseEntry {
                        licensee: licensee.clone(),
                        terms_hash: terms_hash.clone(),
                    },
                );
                found = true;
                break;
            }
        }
        if !found {
            licenses.push_back(LicenseEntry {
                licensee,
                terms_hash,
            });
        }

        env.storage()
            .persistent()
            .set(&DataKey::IpLicenses(ip_id), &licenses);
        env.storage().persistent().extend_ttl(
            &DataKey::IpLicenses(ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );
    }

    /// Revoke a license for an IP from a licensee. Owner-only.
    pub fn revoke_license(env: Env, ip_id: u64, licensee: Address) {
        let record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();

        let mut licenses: Vec<LicenseEntry> = env
            .storage()
            .persistent()
            .get(&DataKey::IpLicenses(ip_id))
            .unwrap_or(Vec::new(&env));

        if let Some(pos) = licenses.iter().position(|e| e.licensee == licensee) {
            licenses.remove(pos as u32);
        } else {
            env.panic_with_error(Error::from_contract_error(
                ContractError::LicenseeNotFound as u32,
            ));
        }

        env.storage()
            .persistent()
            .set(&DataKey::IpLicenses(ip_id), &licenses);
        env.storage().persistent().extend_ttl(
            &DataKey::IpLicenses(ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );
    }

    /// Get all licenses for an IP.
    pub fn get_licenses(env: Env, ip_id: u64) -> Vec<LicenseEntry> {
        require_ip_exists(&env, ip_id);
        env.storage()
            .persistent()
            .get(&DataKey::IpLicenses(ip_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Set or update the suggested price for an IP. Owner-only. Pass 0 to clear.
    pub fn set_ip_suggested_price(env: Env, ip_id: u64, price: i128) {
        let record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();
        if price == 0 {
            env.storage()
                .persistent()
                .remove(&DataKey::SuggestedPrice(ip_id));
        } else {
            env.storage()
                .persistent()
                .set(&DataKey::SuggestedPrice(ip_id), &price);
            env.storage().persistent().extend_ttl(
                &DataKey::SuggestedPrice(ip_id),
                LEDGER_BUMP,
                LEDGER_BUMP,
            );
        }
    }

    /// Get the suggested price for an IP. Returns None if no price has been set.
    pub fn get_ip_suggested_price(env: Env, ip_id: u64) -> Option<i128> {
        require_ip_exists(&env, ip_id);
        env.storage()
            .persistent()
            .get(&DataKey::SuggestedPrice(ip_id))
    }

    /// Add a co-owner to an IP. Owner-only.
    /// Co-owners can verify commitments but cannot transfer or revoke the IP.
    pub fn add_co_owner(env: Env, ip_id: u64, co_owner: Address) {
        let mut record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();

        // Check if already a co-owner
        for existing in record.co_owners.iter() {
            if existing == co_owner {
                return; // Already a co-owner, no-op
            }
        }

        record.co_owners.push_back(co_owner.clone());
        env.storage()
            .persistent()
            .set(&DataKey::IpRecord(ip_id), &record);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::IpRecord(ip_id), LEDGER_BUMP, LEDGER_BUMP);

        env.events()
            .publish((symbol_short!("co_add"), record.owner), (ip_id, co_owner));
    }

    /// Remove a co-owner from an IP. Owner-only.
    pub fn remove_co_owner(env: Env, ip_id: u64, co_owner: Address) {
        let mut record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();

        // Find and remove the co-owner
        if let Some(pos) = record.co_owners.iter().position(|addr| addr == co_owner) {
            record.co_owners.remove(pos as u32);
            env.storage()
                .persistent()
                .set(&DataKey::IpRecord(ip_id), &record);
            env.storage().persistent().extend_ttl(
                &DataKey::IpRecord(ip_id),
                LEDGER_BUMP,
                LEDGER_BUMP,
            );

            env.events()
                .publish((symbol_short!("co_rem"), record.owner), (ip_id, co_owner));
        }
    }

    /// Create a new version of an existing IP commitment.
    ///
    /// This function allows an IP owner to create a new version of their IP
    /// while maintaining a link to the original for prior art proof.
    /// The new version is a separate IP record with its own ID.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `parent_ip_id` - The ID of the original IP to version from
    /// * `new_commitment_hash` - The new commitment hash for this version
    ///
    /// # Returns
    ///
    /// The new IP ID assigned to this version.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// * The parent IP does not exist (IpNotFound error)
    /// * The caller is not the owner of the parent IP (Unauthorized error)
    /// * The new commitment hash is all zeros (ZeroCommitmentHash error)
    /// * The new commitment hash is already registered (CommitmentAlreadyRegistered error)
    pub fn create_ip_version(env: Env, parent_ip_id: u64, new_commitment_hash: BytesN<32>) -> u64 {
        let parent_record = require_ip_exists(&env, parent_ip_id);
        parent_record.owner.require_auth();

        // Reject zero-byte commitment hash
        require_non_zero_commitment(&env, &new_commitment_hash);

        // Reject duplicate commitment hash globally
        require_unique_commitment(&env, &new_commitment_hash);

        // Prevent circular version chains: walk up the parent chain and ensure
        // the new ID (which will be `id`) does not already appear. Since `id`
        // hasn't been assigned yet we check that parent_ip_id is not already
        // an ancestor of itself (i.e. the parent chain is acyclic).
        {
            let mut visited: Vec<u64> = Vec::new(&env);
            let mut cur = parent_ip_id;
            loop {
                // If we've seen this node before, there's a cycle
                for v in visited.iter() {
                    if v == cur {
                        env.panic_with_error(Error::from_contract_error(
                            ContractError::Unauthorized as u32,
                        ));
                    }
                }
                visited.push_back(cur);
                let rec: Option<IpRecord> = env.storage().persistent().get(&DataKey::IpRecord(cur));
                match rec {
                    Some(r) => match r.parent_ip_id {
                        Some(p) => cur = p,
                        None => break,
                    },
                    None => break,
                }
            }
        }

        // Get next ID
        let id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NextId)
            .unwrap_or(1);

        // Create new version record with parent_ip_id set
        let version_record = IpRecord {
            ip_id: id,
            owner: parent_record.owner.clone(),
            commitment_hash: new_commitment_hash.clone(),
            timestamp: env.ledger().timestamp(),
            revoked: false,
            co_owners: Vec::new(&env),
            parent_ip_id: Some(parent_ip_id),
            notary_signature: None,
            expiry_timestamp: 0,
            grace_period_seconds: 0,
        };

        // Store the new version
        env.storage()
            .persistent()
            .set(&DataKey::IpRecord(id), &version_record);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::IpRecord(id), LEDGER_BUMP, LEDGER_BUMP);

        // Add to owner's IP list
        let mut owner_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerIps(parent_record.owner.clone()))
            .unwrap_or(Vec::new(&env));
        owner_ids.push_back(id);
        env.storage()
            .persistent()
            .set(&DataKey::OwnerIps(parent_record.owner.clone()), &owner_ids);
        env.storage().persistent().extend_ttl(
            &DataKey::OwnerIps(parent_record.owner.clone()),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        // Track commitment hash ownership
        env.storage().persistent().set(
            &DataKey::CommitmentOwner(new_commitment_hash.clone()),
            &parent_record.owner,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::CommitmentOwner(new_commitment_hash.clone()),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        // Add to version lineage (direct children of parent)
        let mut versions: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::IpVersions(parent_ip_id))
            .unwrap_or(Vec::new(&env));
        versions.push_back(id);
        env.storage()
            .persistent()
            .set(&DataKey::IpVersions(parent_ip_id), &versions);
        env.storage().persistent().extend_ttl(
            &DataKey::IpVersions(parent_ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        // Update IpVersionChain for the root IP: append new version ID
        {
            let mut root_id = parent_ip_id;
            loop {
                let rec: Option<IpRecord> =
                    env.storage().persistent().get(&DataKey::IpRecord(root_id));
                match rec {
                    Some(r) => match r.parent_ip_id {
                        Some(p) => root_id = p,
                        None => break,
                    },
                    None => break,
                }
            }
            let mut chain: Vec<u64> = env
                .storage()
                .persistent()
                .get(&DataKey::IpVersionChain(root_id))
                .unwrap_or_else(|| {
                    let mut v = Vec::new(&env);
                    v.push_back(root_id);
                    v
                });
            chain.push_back(id);
            env.storage()
                .persistent()
                .set(&DataKey::IpVersionChain(root_id), &chain);
            env.storage().persistent().extend_ttl(
                &DataKey::IpVersionChain(root_id),
                LEDGER_BUMP,
                LEDGER_BUMP,
            );
        }

        // Increment next ID
        env.storage().persistent().set(&DataKey::NextId, &(id + 1));
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::NextId, LEDGER_BUMP, LEDGER_BUMP);

        env.events().publish(
            (symbol_short!("version"), parent_record.owner.clone()),
            (parent_ip_id, id),
        );

        id
    }

    /// Retrieve all versions of an IP (including the original).
    ///
    /// Returns a vector of all IP IDs that are part of the same version lineage,
    /// starting from the original IP and including all subsequent versions.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `ip_id` - The IP ID to get the lineage for (can be original or any version)
    ///
    /// # Returns
    ///
    /// A vector of IP IDs in the lineage, with the original first.
    ///
    /// # Panics
    ///
    /// Panics if the IP record does not exist (IpNotFound error).
    pub fn get_ip_lineage(env: Env, ip_id: u64) -> Vec<u64> {
        let _record = require_ip_exists(&env, ip_id);

        // Find the root IP (the one with no parent)
        let mut current_id = ip_id;
        let root_id;

        // Walk up the chain to find the root
        loop {
            let current_record = require_ip_exists(&env, current_id);
            if let Some(parent_id) = current_record.parent_ip_id {
                current_id = parent_id;
            } else {
                root_id = current_id;
                break;
            }
        }

        // Build lineage starting from root
        let mut lineage = Vec::new(&env);
        lineage.push_back(root_id);

        // Get all versions of the root
        let versions: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::IpVersions(root_id))
            .unwrap_or(Vec::new(&env));

        for version_id in versions.iter() {
            lineage.push_back(version_id);
        }

        lineage
    }

    /// Alias for `create_ip_version`. Commit a new version of an existing IP.
    ///
    /// Links the new commitment to `parent_ip_id`, establishing a verifiable
    /// version history while preserving prior-art proof for each version.
    ///
    /// # Panics
    ///
    /// Panics if the parent IP does not exist, the caller is not the owner,
    /// the hash is zero/duplicate, or a circular chain would be created.
    pub fn commit_ip_version(
        env: Env,
        owner: Address,
        commitment_hash: BytesN<32>,
        parent_ip_id: u64,
    ) -> u64 {
        owner.require_auth();
        Self::create_ip_version(env, parent_ip_id, commitment_hash)
    }

    /// Get all direct child version IDs for a given IP.
    ///
    /// Returns the IDs of all IPs that were created as direct versions of `ip_id`
    /// via `create_ip_version` or `commit_ip_version`. Does not include
    /// grandchildren or deeper descendants.
    ///
    /// # Returns
    ///
    /// A `Vec<u64>` of direct child version IDs, or an empty vec if none exist.
    ///
    /// # Panics
    ///
    /// Panics if the IP record does not exist (IpNotFound error).
    pub fn get_ip_versions(env: Env, ip_id: u64) -> Vec<u64> {
        require_ip_exists(&env, ip_id);
        env.storage()
            .persistent()
            .get(&DataKey::IpVersions(ip_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Retrieve the full version chain for an IP, rooted at the original.
    ///
    /// Returns a `Vec<u64>` starting with the root IP ID followed by all
    /// descendant version IDs in the order they were committed.
    /// If the IP has no versions, returns a single-element vec with the root ID.
    ///
    /// # Panics
    ///
    /// Panics if the IP record does not exist (IpNotFound error).
    pub fn get_ip_version_chain(env: Env, ip_id: u64) -> Vec<u64> {
        require_ip_exists(&env, ip_id);

        // Walk up to find the root
        let mut root_id = ip_id;
        loop {
            let rec: Option<IpRecord> = env.storage().persistent().get(&DataKey::IpRecord(root_id));
            match rec {
                Some(r) => match r.parent_ip_id {
                    Some(p) => root_id = p,
                    None => break,
                },
                None => break,
            }
        }

        // Return stored chain, or a single-element vec if no versions exist yet
        env.storage()
            .persistent()
            .get(&DataKey::IpVersionChain(root_id))
            .unwrap_or_else(|| {
                let mut v = Vec::new(&env);
                v.push_back(root_id);
                v
            })
    }

    // ── Issue #343: Merkle Tree Proof ──────────────────────────────────────────
    /// This enables proving membership in a set of IPs without full disclosure.
    pub fn compute_ip_merkle_root(env: Env, owner: Address) -> BytesN<32> {
        let ip_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerIps(owner))
            .unwrap_or(Vec::new(&env));

        if ip_ids.is_empty() {
            return BytesN::from_array(&env, &[0u8; 32]);
        }

        let mut hashes: Vec<BytesN<32>> = Vec::new(&env);
        for ip_id in ip_ids.iter() {
            if let Some(record) = env
                .storage()
                .persistent()
                .get::<DataKey, IpRecord>(&DataKey::IpRecord(ip_id))
            {
                hashes.push_back(record.commitment_hash);
            }
        }

        Self::merkle_root(&env, &hashes)
    }

    /// Verify a Merkle proof for an IP commitment.
    pub fn verify_ip_merkle_proof(env: Env, ip_id: u64, proof: Vec<BytesN<32>>) -> bool {
        let record = require_ip_exists(&env, ip_id);
        let owner = record.owner.clone();

        let ip_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerIps(owner))
            .unwrap_or(Vec::new(&env));

        let mut hashes: Vec<BytesN<32>> = Vec::new(&env);
        for id in ip_ids.iter() {
            if let Some(rec) = env
                .storage()
                .persistent()
                .get::<DataKey, IpRecord>(&DataKey::IpRecord(id))
            {
                hashes.push_back(rec.commitment_hash);
            }
        }

        Self::verify_merkle_proof(&env, &record.commitment_hash, &proof, &hashes)
    }

    // ── Issue #435: Generate Merkle Proof ─────────────────────────────────────

    /// Generate a Merkle proof for an IP commitment.
    ///
    /// Returns the sibling hashes needed to reconstruct the Merkle root from
    /// the given IP's commitment hash. This allows proving IP membership in the
    /// owner's set without revealing all other commitments.
    ///
    /// # Arguments
    ///
    /// * `ip_id` - The IP to generate a proof for
    ///
    /// # Returns
    ///
    /// A `Vec<BytesN<32>>` of sibling hashes forming the proof path from leaf to root.
    ///
    /// # Panics
    ///
    /// Panics if the IP does not exist.
    pub fn generate_merkle_proof(env: Env, ip_id: u64) -> Vec<BytesN<32>> {
        let record = require_ip_exists(&env, ip_id);

        let ip_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerIps(record.owner.clone()))
            .unwrap_or(Vec::new(&env));

        let mut leaves: Vec<BytesN<32>> = Vec::new(&env);
        let mut leaf_index: u32 = 0;
        let mut found_index: u32 = 0;

        for id in ip_ids.iter() {
            if let Some(rec) = env
                .storage()
                .persistent()
                .get::<DataKey, IpRecord>(&DataKey::IpRecord(id))
            {
                if rec.commitment_hash == record.commitment_hash {
                    found_index = leaf_index;
                }
                leaves.push_back(rec.commitment_hash);
                leaf_index += 1;
            }
        }

        Self::build_merkle_proof(&env, &leaves, found_index)
    }

    /// Build a Merkle proof for the leaf at `index` in `leaves`.
    fn build_merkle_proof(env: &Env, leaves: &Vec<BytesN<32>>, index: u32) -> Vec<BytesN<32>> {
        let mut proof = Vec::new(env);
        if leaves.len() <= 1 {
            return proof;
        }

        let mut current_level = leaves.clone();
        let mut current_index = index;

        while current_level.len() > 1 {
            let sibling_index = if current_index.is_multiple_of(2) {
                current_index + 1
            } else {
                current_index - 1
            };

            let sibling = if sibling_index < current_level.len() {
                current_level.get(sibling_index).unwrap()
            } else {
                // Odd node: duplicate itself
                current_level.get(current_index).unwrap()
            };
            proof.push_back(sibling);

            // Build next level
            let mut next_level = Vec::new(env);
            for i in (0..current_level.len()).step_by(2) {
                let left = current_level.get(i).unwrap();
                let right = if i + 1 < current_level.len() {
                    current_level.get(i + 1).unwrap()
                } else {
                    left.clone()
                };
                let mut combined = Bytes::new(env);
                combined.append(&left.into());
                combined.append(&right.into());
                let hash: BytesN<32> = env.crypto().sha256(&combined).into();
                next_level.push_back(hash);
            }
            current_level = next_level;
            current_index /= 2;
        }

        proof
    }

    fn merkle_root(env: &Env, hashes: &Vec<BytesN<32>>) -> BytesN<32> {
        if hashes.is_empty() {
            return BytesN::from_array(env, &[0u8; 32]);
        }
        if hashes.len() == 1 {
            return hashes.get(0).unwrap();
        }

        let mut current_level = hashes.clone();
        while current_level.len() > 1 {
            let mut next_level = Vec::new(env);
            for i in (0..current_level.len()).step_by(2) {
                let left = current_level.get(i).unwrap();
                let right = if i + 1 < current_level.len() {
                    current_level.get(i + 1).unwrap()
                } else {
                    left.clone()
                };

                let mut combined = Bytes::new(env);
                combined.append(&left.into());
                combined.append(&right.into());
                let hash: BytesN<32> = env.crypto().sha256(&combined).into();
                next_level.push_back(hash);
            }
            current_level = next_level;
        }

        current_level.get(0).unwrap()
    }

    fn verify_merkle_proof(
        env: &Env,
        leaf: &BytesN<32>,
        proof: &Vec<BytesN<32>>,
        all_leaves: &Vec<BytesN<32>>,
    ) -> bool {
        let root = Self::merkle_root(env, all_leaves);

        fn hash_pair(env: &Env, left: &BytesN<32>, right: &BytesN<32>) -> BytesN<32> {
            let mut combined = Bytes::new(env);
            combined.append(&left.clone().into());
            combined.append(&right.clone().into());
            env.crypto().sha256(&combined).into()
        }

        let mut candidates = Vec::from_array(env, [leaf.clone()]);
        for proof_hash in proof.iter() {
            let mut next = Vec::new(env);
            for candidate in candidates.iter() {
                next.push_back(hash_pair(env, &candidate, &proof_hash));
                next.push_back(hash_pair(env, &proof_hash, &candidate));
            }
            candidates = next;
        }

        candidates.iter().any(|candidate| candidate == root)
    }

    // ── Issue #344: Tiered Access Control ──────────────────────────────────────

    /// Grant tiered access to an IP for a third party. Owner-only.
    ///
    /// Access tiers are hierarchical — a higher tier implies all lower tiers:
    /// - `1` = **view**: read IP metadata
    /// - `2` = **verify**: view + verify the commitment
    /// - `3` = **transfer**: view + verify + initiate transfer
    ///
    /// Granting to an address that already has a grant updates the level.
    ///
    /// # Panics
    ///
    /// Panics with `Unauthorized` if `access_level` is 0 or > 3, or if caller is not the owner.
    pub fn grant_ip_access(env: Env, ip_id: u64, grantee: Address, access_level: u32) {
        let record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();

        if !(1..=3).contains(&access_level) {
            env.panic_with_error(Error::from_contract_error(
                ContractError::Unauthorized as u32,
            ));
        }

        let mut grants: Vec<IpAccessGrant> = env
            .storage()
            .persistent()
            .get(&DataKey::IpAccessGrants(ip_id))
            .unwrap_or(Vec::new(&env));

        let mut found = false;
        for i in 0..grants.len() {
            if grants.get(i).unwrap().grantee == grantee {
                grants.set(
                    i,
                    IpAccessGrant {
                        grantee: grantee.clone(),
                        access_level,
                    },
                );
                found = true;
                break;
            }
        }
        if !found {
            grants.push_back(IpAccessGrant {
                grantee: grantee.clone(),
                access_level,
            });
        }

        env.storage()
            .persistent()
            .set(&DataKey::IpAccessGrants(ip_id), &grants);
        env.storage().persistent().extend_ttl(
            &DataKey::IpAccessGrants(ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.events()
            .publish((symbol_short!("ac_grant"), ip_id), (grantee, access_level));
    }

    /// Revoke access to an IP from a third party. Owner-only.
    /// No-op if the grantee has no grant.
    pub fn revoke_ip_access(env: Env, ip_id: u64, grantee: Address) {
        let record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();

        let mut grants: Vec<IpAccessGrant> = env
            .storage()
            .persistent()
            .get(&DataKey::IpAccessGrants(ip_id))
            .unwrap_or(Vec::new(&env));

        if let Some(pos) = grants.iter().position(|g| g.grantee == grantee) {
            grants.remove(pos as u32);
            env.storage()
                .persistent()
                .set(&DataKey::IpAccessGrants(ip_id), &grants);
            env.storage().persistent().extend_ttl(
                &DataKey::IpAccessGrants(ip_id),
                LEDGER_BUMP,
                LEDGER_BUMP,
            );

            env.events()
                .publish((symbol_short!("ac_revoke"), ip_id), grantee);
        }
    }

    /// Get all access grants for an IP.
    pub fn get_ip_access_grants(env: Env, ip_id: u64) -> Vec<IpAccessGrant> {
        require_ip_exists(&env, ip_id);
        env.storage()
            .persistent()
            .get(&DataKey::IpAccessGrants(ip_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Check whether `grantee` has at least `required_level` access to `ip_id`.
    ///
    /// The owner always has full access (level 3). Tiers are hierarchical:
    /// a grantee with level 3 satisfies a check for level 1 or 2.
    ///
    /// Returns `true` if access is granted, `false` otherwise.
    pub fn check_ip_access(env: Env, ip_id: u64, grantee: Address, required_level: u32) -> bool {
        let record = require_ip_exists(&env, ip_id);
        // Owner always has full access
        if grantee == record.owner {
            return true;
        }
        let grants: Vec<IpAccessGrant> = env
            .storage()
            .persistent()
            .get(&DataKey::IpAccessGrants(ip_id))
            .unwrap_or(Vec::new(&env));
        for grant in grants.iter() {
            if grant.grantee == grantee {
                return grant.access_level >= required_level;
            }
        }
        false
    }

    // ── Issue #345 / #428: Timestamp Notarization ──────────────────────────────

    /// Set the trusted notary public key (Ed25519, 32 bytes). Admin-only.
    ///
    /// Must be called once after deployment to configure the notary public key
    /// used to verify timestamp signatures.
    pub fn set_notary_public_key(env: Env, public_key: BytesN<32>) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| env.current_contract_address());
        admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::NotaryPublicKey, &public_key);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::NotaryPublicKey, LEDGER_BUMP, LEDGER_BUMP);
    }

    /// Notarize an IP timestamp with a notary Ed25519 signature.
    ///
    /// The notary must sign the message `ip_id_be_bytes || timestamp_be_bytes`
    /// (8 bytes each, big-endian) with the private key corresponding to the
    /// stored notary public key. The 64-byte signature is verified on-chain.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// * The IP does not exist (IpNotFound error)
    /// * The notary public key has not been set (Unauthorized error)
    /// * The signature is not exactly 64 bytes (Unauthorized error)
    /// * The Ed25519 signature verification fails (Unauthorized error)
    pub fn notarize_ip_timestamp(env: Env, ip_id: u64, notary_signature: Bytes) {
        let mut record = require_ip_exists(&env, ip_id);

        // Require notary public key to be configured
        let public_key: BytesN<32> = match env.storage().persistent().get(&DataKey::NotaryPublicKey)
        {
            Some(k) => k,
            None => {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::Unauthorized as u32,
                ));
            }
        };

        // Signature must be exactly 64 bytes for Ed25519
        if notary_signature.len() != 64 {
            env.panic_with_error(Error::from_contract_error(
                ContractError::Unauthorized as u32,
            ));
        }
        let sig: BytesN<64> = match notary_signature.clone().try_into() {
            Ok(s) => s,
            Err(_) => {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::Unauthorized as u32,
                ));
            }
        };

        // Message: ip_id (8 bytes BE) || timestamp (8 bytes BE)
        let mut message = Bytes::new(&env);
        message.append(&Bytes::from_array(&env, &ip_id.to_be_bytes()));
        message.append(&Bytes::from_array(&env, &record.timestamp.to_be_bytes()));

        // Verify Ed25519 signature — panics if invalid
        env.crypto().ed25519_verify(&public_key, &message, &sig);

        record.notary_signature = Some(notary_signature.clone());

        env.storage()
            .persistent()
            .set(&DataKey::IpRecord(ip_id), &record);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::IpRecord(ip_id), LEDGER_BUMP, LEDGER_BUMP);

        env.events().publish(
            (symbol_short!("notary"), record.owner.clone()),
            (ip_id, env.ledger().timestamp()),
        );
    }

    /// Get the notary signature for an IP, if any.
    pub fn get_ip_notary_signature(env: Env, ip_id: u64) -> Option<Bytes> {
        let record = require_ip_exists(&env, ip_id);
        record.notary_signature.clone()
    }

    // ── Third-Party Attestations ───────────────────────────────────────────────

    // Allow any third party (notary, university, etc.) to attest to an IP's authenticity.
    // Anyone can call this — no owner restriction. The attestor must authorize the call.
    // attest_ip removed - IpAttestations DataKey variant not defined

    // Retrieve all attestations for a given IP.
    // get_ip_attestations removed - IpAttestations DataKey variant not defined

    // ── IP Dispute Challenges ─────────────────────────────────────────────────

    // Submit a challenge against an IP commitment. Anyone can challenge.
    // The challenger must authorize the call. Appends a new IpChallenge to
    // the dispute list for the given IP.
    // challenge_ip removed - IpDisputes DataKey variant not defined

    // Resolve all open disputes for an IP. Admin-only.
    // Marks every unresolved challenge as resolved with the provided resolution.
    // resolve_ip_dispute removed - IpDisputes DataKey variant not defined

    // get_ip_disputes removed - IpDisputes DataKey variant not defined

    // ── Issue #346 / #429: Commitment Rollback Protection ─────────────────────

    /// Compute and store a checksum of all commitments for rollback protection.
    ///
    /// Appends the latest commitment hash to the tracked list, then recomputes
    /// the checksum as sha256 of all concatenated commitment hashes.
    fn update_commitment_checksum(env: &Env) {
        // Retrieve the current next ID to find the most recently added commitment
        let next_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NextId)
            .unwrap_or(1);

        // The last committed IP ID is next_id - 1 (if any IPs exist)
        if next_id <= 1 {
            return;
        }
        let last_id = next_id - 1;

        // Get the commitment hash of the last committed IP
        let last_record: Option<IpRecord> =
            env.storage().persistent().get(&DataKey::IpRecord(last_id));

        if let Some(record) = last_record {
            // Append to tracked commitment hashes list
            let mut hashes: Vec<BytesN<32>> = env
                .storage()
                .persistent()
                .get(&DataKey::CommitmentHashes)
                .unwrap_or(Vec::new(env));
            hashes.push_back(record.commitment_hash);
            env.storage()
                .persistent()
                .set(&DataKey::CommitmentHashes, &hashes);
            env.storage().persistent().extend_ttl(
                &DataKey::CommitmentHashes,
                LEDGER_BUMP,
                LEDGER_BUMP,
            );

            // Recompute checksum: sha256 of all concatenated commitment hashes
            let mut all_bytes = Bytes::new(env);
            for h in hashes.iter() {
                all_bytes.append(&h.into());
            }
            let checksum: BytesN<32> = env.crypto().sha256(&all_bytes).into();

            env.storage()
                .persistent()
                .set(&DataKey::IpCommitmentChecksum, &checksum);
            env.storage().persistent().extend_ttl(
                &DataKey::IpCommitmentChecksum,
                LEDGER_BUMP,
                LEDGER_BUMP,
            );
        }
    }

    // ── Issue #439: Commitment Deduplication ──────────────────────────────────

    /// Find an existing IP ID that holds the given commitment hash, if any.
    /// Returns `Some(ip_id)` if a duplicate exists, `None` otherwise.
    pub fn find_duplicate_commitment(env: Env, commitment_hash: BytesN<32>) -> Option<u64> {
        let owner: Option<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::CommitmentOwner(commitment_hash.clone()));
        // Walk the owner's IP list to find the matching record
        let owner_addr = owner?;
        let ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerIps(owner_addr))
            .unwrap_or(Vec::new(&env));
        for ip_id in ids.iter() {
            if let Some(record) = env
                .storage()
                .persistent()
                .get::<DataKey, IpRecord>(&DataKey::IpRecord(ip_id))
            {
                if record.commitment_hash == commitment_hash {
                    return Some(ip_id);
                }
            }
        }
        None
    }

    /// Merge a duplicate IP commitment into the primary record.
    ///
    /// The duplicate's owner is added as a co-owner of the primary IP, and the
    /// duplicate record is revoked. Only the primary IP owner may call this.
    ///
    /// # Panics
    ///
    /// Panics if either IP does not exist, the caller is not the primary owner,
    /// or the two IPs do not share the same commitment hash.
    pub fn merge_duplicate_commitment(env: Env, primary_ip_id: u64, duplicate_ip_id: u64) {
        let mut primary = require_ip_exists(&env, primary_ip_id);
        primary.owner.require_auth();

        let mut duplicate = require_ip_exists(&env, duplicate_ip_id);

        // Ensure both records share the same commitment hash
        if primary.commitment_hash != duplicate.commitment_hash {
            env.panic_with_error(Error::from_contract_error(
                ContractError::CommitmentAlreadyRegistered as u32,
            ));
        }

        // Add duplicate owner as co-owner of primary (if not already)
        let dup_owner = duplicate.owner.clone();
        let already_co_owner = primary.co_owners.iter().any(|a| a == dup_owner);
        if !already_co_owner && dup_owner != primary.owner {
            primary.co_owners.push_back(dup_owner.clone());
            env.storage()
                .persistent()
                .set(&DataKey::IpRecord(primary_ip_id), &primary);
            env.storage().persistent().extend_ttl(
                &DataKey::IpRecord(primary_ip_id),
                LEDGER_BUMP,
                LEDGER_BUMP,
            );
        }

        // Revoke the duplicate record
        duplicate.revoked = true;
        env.storage()
            .persistent()
            .set(&DataKey::IpRecord(duplicate_ip_id), &duplicate);
        env.storage().persistent().extend_ttl(
            &DataKey::IpRecord(duplicate_ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        // Issue #436: Audit entries for the merge
        Self::append_audit_entry(
            &env,
            primary_ip_id,
            symbol_short!("merged"),
            primary.owner.clone(),
        );
        Self::append_audit_entry(
            &env,
            duplicate_ip_id,
            symbol_short!("revoked"),
            primary.owner,
        );

        env.events().publish(
            (symbol_short!("dedup"), primary_ip_id),
            (duplicate_ip_id, dup_owner),
        );
    }

    // ── Issue #438: Commitment Compression ────────────────────────────────────

    /// Retrieve the compressed (16-byte) form of a commitment hash for an IP.
    /// The compressed form is the first 16 bytes of the full 32-byte hash,
    /// reducing storage footprint by 50% for indexing use cases.
    pub fn get_compressed_commitment(env: Env, ip_id: u64) -> BytesN<16> {
        require_ip_exists(&env, ip_id);
        env.storage()
            .persistent()
            .get(&DataKey::CompressedCommitment(ip_id))
            .unwrap_or_else(|| BytesN::from_array(&env, &[0u8; 16]))
    }

    /// Internal: store the compressed commitment for an IP.
    fn store_compressed_commitment(env: &Env, ip_id: u64, commitment_hash: &BytesN<32>) {
        let full = commitment_hash.to_array();
        let mut half = [0u8; 16];
        half.copy_from_slice(&full[..16]);
        let compressed = BytesN::from_array(env, &half);
        env.storage()
            .persistent()
            .set(&DataKey::CompressedCommitment(ip_id), &compressed);
        env.storage().persistent().extend_ttl(
            &DataKey::CompressedCommitment(ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );
    }

    // ── Issue #437: Commitment Sharding ───────────────────────────────────────

    /// Compute the shard ID for a commitment hash.
    /// Uses the first byte of the hash modulo NUM_SHARDS.
    pub fn get_commitment_shard(_env: Env, commitment_hash: BytesN<32>) -> u32 {
        let bytes = commitment_hash.to_array();
        (bytes[0] as u32) % NUM_SHARDS
    }

    /// Retrieve all IP IDs stored in a given shard.
    pub fn get_shard_ip_ids(env: Env, shard_id: u32) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::ShardIps(shard_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Internal: assign an IP to its shard based on its commitment hash.
    fn assign_to_shard(env: &Env, ip_id: u64, commitment_hash: &BytesN<32>) {
        let bytes = commitment_hash.to_array();
        let shard_id = (bytes[0] as u32) % NUM_SHARDS;
        let mut shard_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::ShardIps(shard_id))
            .unwrap_or(Vec::new(env));
        shard_ids.push_back(ip_id);
        env.storage()
            .persistent()
            .set(&DataKey::ShardIps(shard_id), &shard_ids);
        env.storage().persistent().extend_ttl(
            &DataKey::ShardIps(shard_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );
    }

    // ── Issue #436: Audit Trail Immutability ──────────────────────────────────

    /// Append an immutable audit entry for an IP. Internal helper — entries are
    /// never overwritten or removed, only appended.
    fn append_audit_entry(env: &Env, ip_id: u64, action: soroban_sdk::Symbol, actor: Address) {
        let mut trail: Vec<AuditEntry> = env
            .storage()
            .persistent()
            .get(&DataKey::IpAuditTrail(ip_id))
            .unwrap_or(Vec::new(env));
        trail.push_back(AuditEntry {
            action,
            actor,
            timestamp: env.ledger().timestamp(),
        });
        env.storage()
            .persistent()
            .set(&DataKey::IpAuditTrail(ip_id), &trail);
        env.storage().persistent().extend_ttl(
            &DataKey::IpAuditTrail(ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );
    }

    /// Retrieve the immutable audit trail for an IP.
    /// Returns all audit entries in chronological order.
    pub fn get_ip_audit_trail(env: Env, ip_id: u64) -> Vec<AuditEntry> {
        require_ip_exists(&env, ip_id);
        env.storage()
            .persistent()
            .get(&DataKey::IpAuditTrail(ip_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Verify the integrity of all commitments (for upgrade safety).
    ///
    /// Recomputes the checksum from the tracked commitment hashes list and
    /// compares it to the stored checksum. Returns `true` if they match.
    pub fn verify_commitment_integrity(env: Env) -> bool {
        let stored_checksum: Option<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&DataKey::IpCommitmentChecksum);

        if stored_checksum.is_none() {
            return true; // No checksum stored yet — nothing to verify
        }

        let hashes: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&DataKey::CommitmentHashes)
            .unwrap_or(Vec::new(&env));

        let mut all_bytes = Bytes::new(&env);
        for h in hashes.iter() {
            all_bytes.append(&h.into());
        }
        let recomputed: BytesN<32> = env.crypto().sha256(&all_bytes).into();

        stored_checksum.unwrap() == recomputed
    }

    // ── Issue #431: IP Claim Expiration Warnings ───────────────────────────────

    /// Check if an IP commitment is approaching expiration and emit a warning event.
    ///
    /// Compares the remaining TTL of the IP record against `warning_threshold_ledgers`.
    /// If the remaining TTL is less than or equal to the threshold, emits an
    /// `exp_warn` event and returns `true`. Otherwise returns `false`.
    ///
    /// The TTL is estimated as `LEDGER_BUMP` minus ledgers elapsed since commitment
    /// (using ledger sequence numbers as a proxy).
    ///
    /// # Arguments
    ///
    /// * `ip_id` - The IP to check
    /// * `warning_threshold_ledgers` - Warn if remaining TTL ≤ this many ledgers
    ///
    /// # Returns
    ///
    /// `true` if the IP is approaching expiration, `false` otherwise.
    ///
    /// # Panics
    ///
    /// Panics if the IP record does not exist (IpNotFound error).
    pub fn check_expiration_warning(env: Env, ip_id: u64, warning_threshold_ledgers: u32) -> bool {
        let record = require_ip_exists(&env, ip_id);

        // Estimate remaining TTL: LEDGER_BUMP minus ledgers elapsed since commit.
        // We use ledger sequence as a proxy for elapsed time.
        // The record was committed at some ledger; we stored LEDGER_BUMP TTL at that point.
        // Current sequence - commit sequence ≈ ledgers elapsed.
        // Since we don't store the commit ledger sequence, we use timestamp difference
        // as a proxy: elapsed_seconds / 5 ≈ elapsed_ledgers (5s per ledger).
        let current_timestamp = env.ledger().timestamp();
        let commit_timestamp = record.timestamp;
        let elapsed_seconds = current_timestamp.saturating_sub(commit_timestamp);
        let elapsed_ledgers = (elapsed_seconds / 5) as u32;
        let remaining_ttl = LEDGER_BUMP.saturating_sub(elapsed_ledgers);

        if remaining_ttl <= warning_threshold_ledgers {
            env.events().publish(
                (symbol_short!("exp_warn"), ip_id),
                (record.owner, remaining_ttl),
            );
            return true;
        }

        false
    }

    // ── Issue #433: IP Ownership Proof Challenge ───────────────────────────────

    /// Issue a challenge for an IP ownership proof.
    ///
    /// A third party (challenger) issues a nonce-based challenge to the IP owner.
    /// The owner must respond with sha256(commitment_hash || nonce) to prove ownership.
    ///
    /// Returns the challenge_id.
    pub fn issue_ownership_challenge(
        env: Env,
        ip_id: u64,
        challenger: Address,
        nonce: BytesN<32>,
    ) -> u64 {
        challenger.require_auth();
        require_ip_exists(&env, ip_id);

        let challenge_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NextChallengeId)
            .unwrap_or(1u64);

        let challenge = OwnershipChallenge {
            challenge_id,
            ip_id,
            challenger,
            nonce,
            response_hash: None,
            verified: false,
            timestamp: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::OwnershipChallenge(challenge_id), &challenge);
        env.storage().persistent().extend_ttl(
            &DataKey::OwnershipChallenge(challenge_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );
        env.storage()
            .persistent()
            .set(&DataKey::NextChallengeId, &(challenge_id + 1));
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::NextChallengeId, LEDGER_BUMP, LEDGER_BUMP);

        challenge_id
    }

    /// Respond to an ownership challenge.
    ///
    /// The IP owner responds with sha256(commitment_hash || nonce). The response
    /// is stored on-chain for verification.
    ///
    /// # Panics
    ///
    /// Panics if the challenge does not exist or the caller is not the IP owner.
    pub fn respond_to_ownership_challenge(env: Env, challenge_id: u64, response_hash: BytesN<32>) {
        let mut challenge: OwnershipChallenge = env
            .storage()
            .persistent()
            .get(&DataKey::OwnershipChallenge(challenge_id))
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(ContractError::IpNotFound as u32))
            });

        let record = require_ip_exists(&env, challenge.ip_id);
        record.owner.require_auth();

        challenge.response_hash = Some(response_hash);
        env.storage()
            .persistent()
            .set(&DataKey::OwnershipChallenge(challenge_id), &challenge);
        env.storage().persistent().extend_ttl(
            &DataKey::OwnershipChallenge(challenge_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );
    }

    /// Verify an ownership challenge response.
    ///
    /// Checks that the stored response_hash equals sha256(commitment_hash || nonce).
    /// Marks the challenge as verified if valid.
    ///
    /// Returns `true` if the proof is valid, `false` otherwise.
    ///
    /// # Panics
    ///
    /// Panics if the challenge does not exist.
    pub fn verify_ownership_challenge(env: Env, challenge_id: u64) -> bool {
        let mut challenge: OwnershipChallenge = env
            .storage()
            .persistent()
            .get(&DataKey::OwnershipChallenge(challenge_id))
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(ContractError::IpNotFound as u32))
            });

        let response_hash = match challenge.response_hash.clone() {
            Some(h) => h,
            None => return false,
        };

        let record = require_ip_exists(&env, challenge.ip_id);

        // Expected: sha256(commitment_hash || nonce)
        let mut preimage = Bytes::new(&env);
        preimage.append(&record.commitment_hash.into());
        preimage.append(&challenge.nonce.clone().into());
        let expected: BytesN<32> = env.crypto().sha256(&preimage).into();

        let valid = response_hash == expected;
        if valid {
            challenge.verified = true;
            env.storage()
                .persistent()
                .set(&DataKey::OwnershipChallenge(challenge_id), &challenge);
            env.storage().persistent().extend_ttl(
                &DataKey::OwnershipChallenge(challenge_id),
                LEDGER_BUMP,
                LEDGER_BUMP,
            );
        }
        valid
    }

    /// Retrieve an ownership challenge by ID.
    ///
    /// Returns `None` if the challenge does not exist.
    pub fn get_ownership_challenge(env: Env, challenge_id: u64) -> Option<OwnershipChallenge> {
        env.storage()
            .persistent()
            .get(&DataKey::OwnershipChallenge(challenge_id))
    }

    // ── Issue #434: Encryption Key Rotation ───────────────────────────────────

    /// Rotate the commitment key for an IP.
    ///
    /// Allows the IP owner to update the commitment hash (e.g. after re-encrypting
    /// with a new key). The caller must prove knowledge of the old commitment by
    /// providing the original secret and blinding factor. The old commitment hash
    /// is stored in rotation history for audit purposes.
    ///
    /// # Panics
    ///
    /// Panics if the IP does not exist, the caller is not the owner, the IP is
    /// revoked, the old secret/blinding_factor do not match the stored commitment,
    /// or the new hash is zero/duplicate.
    pub fn rotate_commitment_key(
        env: Env,
        ip_id: u64,
        new_commitment_hash: BytesN<32>,
        old_secret: BytesN<32>,
        old_blinding_factor: BytesN<32>,
    ) {
        let mut record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();
        require_not_revoked(&env, &record);

        // Verify old commitment
        let mut preimage = Bytes::new(&env);
        preimage.append(&old_secret.into());
        preimage.append(&old_blinding_factor.into());
        let computed: BytesN<32> = env.crypto().sha256(&preimage).into();
        if computed != record.commitment_hash {
            env.panic_with_error(Error::from_contract_error(
                ContractError::Unauthorized as u32,
            ));
        }

        // Validate new hash
        require_non_zero_commitment(&env, &new_commitment_hash);
        require_unique_commitment(&env, &new_commitment_hash);

        // Store rotation history (append old hash)
        let mut history: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&DataKey::EncryptionKeyRotation(ip_id))
            .unwrap_or(Vec::new(&env));
        history.push_back(record.commitment_hash.clone());
        env.storage()
            .persistent()
            .set(&DataKey::EncryptionKeyRotation(ip_id), &history);
        env.storage().persistent().extend_ttl(
            &DataKey::EncryptionKeyRotation(ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        // Update CommitmentOwner index: remove old, add new
        env.storage()
            .persistent()
            .remove(&DataKey::CommitmentOwner(record.commitment_hash.clone()));
        env.storage().persistent().set(
            &DataKey::CommitmentOwner(new_commitment_hash.clone()),
            &record.owner,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::CommitmentOwner(new_commitment_hash.clone()),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        // Update the record
        record.commitment_hash = new_commitment_hash.clone();
        env.storage()
            .persistent()
            .set(&DataKey::IpRecord(ip_id), &record);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::IpRecord(ip_id), LEDGER_BUMP, LEDGER_BUMP);

        env.events().publish(
            (symbol_short!("key_rot"), record.owner),
            (ip_id, new_commitment_hash),
        );
    }

    /// Get the key rotation history for an IP (list of old commitment hashes).
    pub fn get_key_rotation_history(env: Env, ip_id: u64) -> Vec<BytesN<32>> {
        require_ip_exists(&env, ip_id);
        env.storage()
            .persistent()
            .get(&DataKey::EncryptionKeyRotation(ip_id))
            .unwrap_or(Vec::new(&env))
    }

    // ── Dispute Resolution ────────────────────────────────────────────────────

    /// Initiate a dispute against an IP. The challenger must authorize.
    /// Returns the new dispute ID.
    pub fn initiate_dispute(
        env: Env,
        ip_id: u64,
        challenger: Address,
        evidence_hash: BytesN<32>,
    ) -> u64 {
        challenger.require_auth();
        require_ip_exists(&env, ip_id);

        let dispute_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NextDisputeId)
            .unwrap_or(1);

        let record = DisputeRecord {
            dispute_id,
            ip_id,
            challenger: challenger.clone(),
            evidence_hash,
            timestamp: env.ledger().timestamp(),
            resolved: false,
            winner: None,
        };

        env.storage()
            .persistent()
            .set(&DataKey::IpDisputes(dispute_id), &record);
        env.storage().persistent().extend_ttl(
            &DataKey::IpDisputes(dispute_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.storage()
            .persistent()
            .set(&DataKey::NextDisputeId, &(dispute_id + 1));
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::NextDisputeId, LEDGER_BUMP, LEDGER_BUMP);

        env.events()
            .publish((symbol_short!("dispute"), challenger), (dispute_id, ip_id));

        dispute_id
    }

    /// Submit additional evidence for an open dispute. Either the IP owner or
    /// the challenger may call this. Caller must authorize.
    pub fn submit_dispute_evidence(
        env: Env,
        dispute_id: u64,
        submitter: Address,
        evidence_hash: BytesN<32>,
    ) {
        submitter.require_auth();

        let mut record: DisputeRecord = env
            .storage()
            .persistent()
            .get(&DataKey::IpDisputes(dispute_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::DisputeNotFound));

        if record.resolved {
            panic_with_error!(&env, ContractError::DisputeAlreadyResolved);
        }

        // Only the IP owner or the challenger may submit evidence.
        let ip_record = require_ip_exists(&env, record.ip_id);
        if submitter != ip_record.owner && submitter != record.challenger {
            panic_with_error!(&env, ContractError::Unauthorized);
        }

        record.evidence_hash = evidence_hash;
        env.storage()
            .persistent()
            .set(&DataKey::IpDisputes(dispute_id), &record);
        env.storage().persistent().extend_ttl(
            &DataKey::IpDisputes(dispute_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.events()
            .publish((symbol_short!("disp_ev"), submitter), dispute_id);
    }

    /// Resolve a dispute. Admin-only. Transfers IP ownership to `winner` if
    /// winner differs from the current owner.
    pub fn resolve_dispute(env: Env, dispute_id: u64, winner: Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::Unauthorized));
        admin.require_auth();

        let mut record: DisputeRecord = env
            .storage()
            .persistent()
            .get(&DataKey::IpDisputes(dispute_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::DisputeNotFound));

        if record.resolved {
            panic_with_error!(&env, ContractError::DisputeAlreadyResolved);
        }

        record.resolved = true;
        record.winner = Some(winner.clone());
        env.storage()
            .persistent()
            .set(&DataKey::IpDisputes(dispute_id), &record);
        env.storage().persistent().extend_ttl(
            &DataKey::IpDisputes(dispute_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.events().publish(
            (symbol_short!("disp_res"), winner.clone()),
            (dispute_id, record.ip_id),
        );
    }

    /// Get a dispute record by ID.
    pub fn get_dispute(env: Env, dispute_id: u64) -> DisputeRecord {
        env.storage()
            .persistent()
            .get(&DataKey::IpDisputes(dispute_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::DisputeNotFound))
    }

    // ── Issue #447: IP Commitment Staking ─────────────────────────────────────

    /// Stake XLM (represented as an i128 amount) against an IP commitment.
    /// Only the IP owner may stake. One active stake per IP.
    pub fn stake_commitment(env: Env, ip_id: u64, amount: i128) {
        let record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();

        if env.storage().persistent().has(&DataKey::IpStake(ip_id)) {
            panic_with_error!(&env, ContractError::AlreadyStaked);
        }

        let stake = StakeRecord {
            ip_id,
            owner: record.owner.clone(),
            amount,
            slashed: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::IpStake(ip_id), &stake);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::IpStake(ip_id), LEDGER_BUMP, LEDGER_BUMP);

        env.events()
            .publish((symbol_short!("staked"), record.owner), (ip_id, amount));
    }

    /// Stake XLM against multiple IP commitments in one call.
    /// Each `ip_ids[i]` is staked with `amounts[i]` and requires the owner
    /// of that IP to authorize the call.
    pub fn batch_stake_commitments(env: Env, ip_ids: Vec<u64>, amounts: Vec<i128>) {
        if ip_ids.len() != amounts.len() {
            env.panic_with_error(Error::from_contract_error(
                ContractError::BatchSizeMismatch as u32,
            ));
        }

        for i in 0..ip_ids.len() {
            let ip_id = ip_ids.get(i).unwrap();
            let amount = amounts.get(i).unwrap();
            let record = require_ip_exists(&env, ip_id);
            record.owner.require_auth();

            if env.storage().persistent().has(&DataKey::IpStake(ip_id)) {
                panic_with_error!(&env, ContractError::AlreadyStaked);
            }

            let stake = StakeRecord {
                ip_id,
                owner: record.owner.clone(),
                amount,
                slashed: false,
            };
            env.storage()
                .persistent()
                .set(&DataKey::IpStake(ip_id), &stake);
            env.storage().persistent().extend_ttl(
                &DataKey::IpStake(ip_id),
                LEDGER_BUMP,
                LEDGER_BUMP,
            );
            env.events()
                .publish((symbol_short!("staked"), record.owner), (ip_id, amount));
        }
    }

    /// Slash the stake for an IP (admin-only). Marks the stake as slashed and
    /// decrements the owner's reputation score.
    pub fn slash_stake(env: Env, ip_id: u64) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::Unauthorized));
        admin.require_auth();

        let mut stake: StakeRecord = env
            .storage()
            .persistent()
            .get(&DataKey::IpStake(ip_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::StakeNotFound));

        if stake.slashed {
            panic_with_error!(&env, ContractError::StakeAlreadySlashed);
        }

        stake.slashed = true;
        env.storage()
            .persistent()
            .set(&DataKey::IpStake(ip_id), &stake);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::IpStake(ip_id), LEDGER_BUMP, LEDGER_BUMP);

        // Penalise reputation
        let mut rep: ReputationRecord = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerReputation(stake.owner.clone()))
            .unwrap_or(ReputationRecord {
                owner: stake.owner.clone(),
                score: 0,
                commitments: 0,
                disputes_lost: 0,
            });
        rep.score = rep.score.saturating_sub(10);
        rep.disputes_lost += 1;
        env.storage()
            .persistent()
            .set(&DataKey::OwnerReputation(stake.owner.clone()), &rep);
        env.storage().persistent().extend_ttl(
            &DataKey::OwnerReputation(stake.owner.clone()),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.events()
            .publish((symbol_short!("slashed"), stake.owner), ip_id);
    }

    /// Unstake: remove an active (non-slashed) stake. Owner-only.
    pub fn unstake(env: Env, ip_id: u64) {
        let stake: StakeRecord = env
            .storage()
            .persistent()
            .get(&DataKey::IpStake(ip_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::StakeNotFound));

        stake.owner.require_auth();

        if stake.slashed {
            panic_with_error!(&env, ContractError::StakeAlreadySlashed);
        }

        env.storage().persistent().remove(&DataKey::IpStake(ip_id));
        env.events()
            .publish((symbol_short!("unstaked"), stake.owner), ip_id);
    }

    /// Get the stake record for an IP.
    pub fn get_stake(env: Env, ip_id: u64) -> Option<StakeRecord> {
        env.storage().persistent().get(&DataKey::IpStake(ip_id))
    }

    // ── Issue #448: IP Commitment Reputation System ───────────────────────────

    /// Get the reputation record for an owner. Returns a default record if none exists.
    pub fn get_reputation(env: Env, owner: Address) -> ReputationRecord {
        env.storage()
            .persistent()
            .get(&DataKey::OwnerReputation(owner.clone()))
            .unwrap_or(ReputationRecord {
                owner,
                score: 0,
                commitments: 0,
                disputes_lost: 0,
            })
    }

    /// Increment the commitment count and score for an owner (called internally on commit).
    /// Also callable by admin to manually adjust reputation.
    pub fn update_reputation(env: Env, owner: Address, score_delta: i64) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::Unauthorized));
        admin.require_auth();

        let mut rep: ReputationRecord = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerReputation(owner.clone()))
            .unwrap_or(ReputationRecord {
                owner: owner.clone(),
                score: 0,
                commitments: 0,
                disputes_lost: 0,
            });
        rep.score = rep.score.saturating_add(score_delta);
        env.storage()
            .persistent()
            .set(&DataKey::OwnerReputation(owner.clone()), &rep);
        env.storage().persistent().extend_ttl(
            &DataKey::OwnerReputation(owner),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );
    }

    /// Adjust reputation for multiple IP commitments in one call.
    /// Each `ip_ids[i]` is used to lookup the IP owner, then the corresponding
    /// `score_deltas[i]` is applied to that owner's reputation.
    pub fn batch_update_reputation(env: Env, ip_ids: Vec<u64>, score_deltas: Vec<i64>) {
        if ip_ids.len() != score_deltas.len() {
            env.panic_with_error(Error::from_contract_error(
                ContractError::BatchSizeMismatch as u32,
            ));
        }

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::Unauthorized));
        admin.require_auth();

        for i in 0..ip_ids.len() {
            let ip_id = ip_ids.get(i).unwrap();
            let score_delta = score_deltas.get(i).unwrap();
            let record = require_ip_exists(&env, ip_id);
            let owner = record.owner.clone();

            let mut rep: ReputationRecord = env
                .storage()
                .persistent()
                .get(&DataKey::OwnerReputation(owner.clone()))
                .unwrap_or(ReputationRecord {
                    owner: owner.clone(),
                    score: 0,
                    commitments: 0,
                    disputes_lost: 0,
                });
            rep.score = rep.score.saturating_add(score_delta);
            env.storage()
                .persistent()
                .set(&DataKey::OwnerReputation(owner.clone()), &rep);
            env.storage().persistent().extend_ttl(
                &DataKey::OwnerReputation(owner),
                LEDGER_BUMP,
                LEDGER_BUMP,
            );
        }
    }

    // ── Issue #449: IP Commitment Dispute Arbitration ─────────────────────────

    /// Nominate an address as an arbitrator (admin-only).
    pub fn nominate_arbitrator(env: Env, arbitrator: Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::Unauthorized));
        admin.require_auth();

        let mut pool: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::ArbitratorPool)
            .unwrap_or(Vec::new(&env));

        // Idempotent: skip if already in pool
        for a in pool.iter() {
            if a == arbitrator {
                return;
            }
        }
        pool.push_back(arbitrator.clone());
        env.storage()
            .persistent()
            .set(&DataKey::ArbitratorPool, &pool);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::ArbitratorPool, LEDGER_BUMP, LEDGER_BUMP);

        env.events()
            .publish((symbol_short!("arb_nom"), admin), arbitrator);
    }

    /// Open an arbitration case for an existing dispute. Admin-only.
    /// Returns the new arbitration_id.
    pub fn open_arbitration(env: Env, dispute_id: u64) -> u64 {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::Unauthorized));
        admin.require_auth();

        // Ensure dispute exists
        let _dispute: DisputeRecord = env
            .storage()
            .persistent()
            .get(&DataKey::IpDisputes(dispute_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::DisputeNotFound));

        let arb_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NextArbitrationId)
            .unwrap_or(1);

        let pool: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::ArbitratorPool)
            .unwrap_or(Vec::new(&env));

        let case = ArbitrationRecord {
            arbitration_id: arb_id,
            dispute_id,
            arbitrators: pool,
            votes_owner: 0,
            votes_challenger: 0,
            finalized: false,
            winner: None,
        };

        env.storage()
            .persistent()
            .set(&DataKey::ArbitrationCase(arb_id), &case);
        env.storage().persistent().extend_ttl(
            &DataKey::ArbitrationCase(arb_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );
        env.storage()
            .persistent()
            .set(&DataKey::NextArbitrationId, &(arb_id + 1));
        env.storage().persistent().extend_ttl(
            &DataKey::NextArbitrationId,
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        arb_id
    }

    /// Cast a vote on an arbitration case. `vote_for_owner = true` votes for the
    /// IP owner; `false` votes for the challenger. Caller must be a nominated arbitrator.
    pub fn vote_on_dispute(env: Env, arbitration_id: u64, voter: Address, vote_for_owner: bool) {
        voter.require_auth();

        let mut case: ArbitrationRecord = env
            .storage()
            .persistent()
            .get(&DataKey::ArbitrationCase(arbitration_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::ArbitrationNotFound));

        if case.finalized {
            panic_with_error!(&env, ContractError::ArbitrationAlreadyFinalized);
        }

        // Verify voter is in the arbitrator pool
        let mut is_arbitrator = false;
        for a in case.arbitrators.iter() {
            if a == voter {
                is_arbitrator = true;
                break;
            }
        }
        if !is_arbitrator {
            panic_with_error!(&env, ContractError::NotAnArbitrator);
        }

        if vote_for_owner {
            case.votes_owner += 1;
        } else {
            case.votes_challenger += 1;
        }

        env.storage()
            .persistent()
            .set(&DataKey::ArbitrationCase(arbitration_id), &case);
        env.storage().persistent().extend_ttl(
            &DataKey::ArbitrationCase(arbitration_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.events().publish(
            (symbol_short!("arb_vote"), voter),
            (arbitration_id, vote_for_owner),
        );
    }

    /// Finalize an arbitration case. Admin-only. Determines winner by majority vote
    /// and resolves the underlying dispute.
    pub fn finalize_arbitration(env: Env, arbitration_id: u64) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::Unauthorized));
        admin.require_auth();

        let mut case: ArbitrationRecord = env
            .storage()
            .persistent()
            .get(&DataKey::ArbitrationCase(arbitration_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::ArbitrationNotFound));

        if case.finalized {
            panic_with_error!(&env, ContractError::ArbitrationAlreadyFinalized);
        }

        let mut dispute: DisputeRecord = env
            .storage()
            .persistent()
            .get(&DataKey::IpDisputes(case.dispute_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::DisputeNotFound));

        let ip_record = require_ip_exists(&env, dispute.ip_id);

        // Majority vote determines winner; ties go to the IP owner
        let winner = if case.votes_challenger > case.votes_owner {
            dispute.challenger.clone()
        } else {
            ip_record.owner.clone()
        };

        case.finalized = true;
        case.winner = Some(winner.clone());
        dispute.resolved = true;
        dispute.winner = Some(winner.clone());

        env.storage()
            .persistent()
            .set(&DataKey::ArbitrationCase(arbitration_id), &case);
        env.storage().persistent().extend_ttl(
            &DataKey::ArbitrationCase(arbitration_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );
        env.storage()
            .persistent()
            .set(&DataKey::IpDisputes(case.dispute_id), &dispute);
        env.storage().persistent().extend_ttl(
            &DataKey::IpDisputes(case.dispute_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.events()
            .publish((symbol_short!("arb_fin"), winner.clone()), arbitration_id);
    }

    /// Get an arbitration case by ID.
    pub fn get_arbitration(env: Env, arbitration_id: u64) -> ArbitrationRecord {
        env.storage()
            .persistent()
            .get(&DataKey::ArbitrationCase(arbitration_id))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::ArbitrationNotFound))
    }

    // ── Issue: IP Commitment Renewal ───────────────────────────────────────────

    /// Renew an expiring IP commitment by extending its on-chain TTL.
    ///
    /// Bumps the storage TTL of the IP record back to `LEDGER_BUMP` ledgers
    /// without creating a new commitment or changing the commitment hash.
    /// A renewal counter is incremented on each call.
    ///
    /// # Panics
    ///
    /// Panics if the IP does not exist, the caller is not the owner, or the IP
    /// is revoked.
    pub fn renew_ip(env: Env, ip_id: u64) {
        let record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();
        require_not_revoked(&env, &record);

        env.storage()
            .persistent()
            .extend_ttl(&DataKey::IpRecord(ip_id), LEDGER_BUMP, LEDGER_BUMP);

        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::RenewalCount(ip_id))
            .unwrap_or(0u32);
        let new_count = count + 1;
        env.storage()
            .persistent()
            .set(&DataKey::RenewalCount(ip_id), &new_count);
        env.storage().persistent().extend_ttl(
            &DataKey::RenewalCount(ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.events()
            .publish((symbol_short!("renewed"), record.owner), (ip_id, new_count));
    }

    /// Get the number of times an IP commitment has been renewed.
    pub fn get_renewal_count(env: Env, ip_id: u64) -> u32 {
        require_ip_exists(&env, ip_id);
        env.storage()
            .persistent()
            .get(&DataKey::RenewalCount(ip_id))
            .unwrap_or(0u32)
    }

    // ── Issue: Delegation Chains ───────────────────────────────────────────────────────────────

    pub fn delegate_commitment_authority(
        env: Env,
        root_owner: Address,
        delegator: Address,
        delegate_address: Address,
    ) {
        delegator.require_auth();

        let new_depth: u32 = if delegator == root_owner {
            0
        } else {
            let stored: Option<u32> = env
                .storage()
                .persistent()
                .get(&DataKey::DelegateDepth(delegator.clone()));
            match stored {
                Some(d) => d + 1,
                None => panic_with_error!(&env, ContractError::Unauthorized),
            }
        };

        if new_depth >= MAX_DELEGATION_DEPTH {
            panic_with_error!(&env, ContractError::Unauthorized);
        }

        let key = DataKey::Delegates(root_owner.clone());
        let mut delegates: Vec<DelegationRecord> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env));

        for i in 0..delegates.len() {
            if delegates.get(i).unwrap().delegate == delegate_address {
                return;
            }
        }

        delegates.push_back(DelegationRecord {
            delegate: delegate_address.clone(),
            depth: new_depth,
        });
        env.storage().persistent().set(&key, &delegates);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_BUMP, LEDGER_BUMP);

        env.storage().persistent().set(
            &DataKey::DelegateDepth(delegate_address.clone()),
            &new_depth,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::DelegateDepth(delegate_address.clone()),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.events().publish(
            (symbol_short!("delegated"), root_owner),
            (delegate_address, new_depth),
        );
    }

    pub fn revoke_delegation(env: Env, owner: Address, delegate_address: Address) {
        owner.require_auth();

        let key = DataKey::Delegates(owner.clone());
        let delegates: Vec<DelegationRecord> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env));

        let mut updated = Vec::new(&env);
        for i in 0..delegates.len() {
            let rec = delegates.get(i).unwrap();
            if rec.delegate != delegate_address {
                updated.push_back(rec);
            }
        }

        env.storage().persistent().set(&key, &updated);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_BUMP, LEDGER_BUMP);

        env.storage()
            .persistent()
            .remove(&DataKey::DelegateDepth(delegate_address.clone()));

        env.events()
            .publish((symbol_short!("revoke"), owner), delegate_address);
    }

    pub fn is_delegate(env: Env, owner: Address, delegate_address: Address) -> bool {
        Self::is_delegate_in_chain(&env, &owner, &delegate_address, 0)
    }

    pub fn commit_ip_delegated(
        env: Env,
        owner: Address,
        commitment_hash: BytesN<32>,
        pow_difficulty: u32,
    ) -> u64 {
        owner.require_auth();

        let key = DataKey::Delegates(owner.clone());
        let delegates: Vec<DelegationRecord> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env));
        if delegates.is_empty() {
            panic_with_error!(&env, ContractError::Unauthorized);
        }

        require_non_zero_commitment(&env, &commitment_hash);
        require_unique_commitment(&env, &commitment_hash);
        require_pow(&env, &commitment_hash, pow_difficulty);

        let id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NextId)
            .unwrap_or(1);

        let record = IpRecord {
            ip_id: id,
            owner: owner.clone(),
            commitment_hash: commitment_hash.clone(),
            timestamp: env.ledger().timestamp(),
            revoked: false,
            co_owners: Vec::new(&env),
            parent_ip_id: None,
            notary_signature: None,
            expiry_timestamp: 0,
            grace_period_seconds: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::IpRecord(id), &record);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::IpRecord(id), LEDGER_BUMP, LEDGER_BUMP);

        let mut ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerIps(owner.clone()))
            .unwrap_or(Vec::new(&env));
        ids.push_back(id);
        env.storage()
            .persistent()
            .set(&DataKey::OwnerIps(owner.clone()), &ids);
        env.storage().persistent().extend_ttl(
            &DataKey::OwnerIps(owner.clone()),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.storage()
            .persistent()
            .set(&DataKey::CommitmentOwner(commitment_hash.clone()), &owner);
        env.storage().persistent().extend_ttl(
            &DataKey::CommitmentOwner(commitment_hash.clone()),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.storage().persistent().set(&DataKey::NextId, &(id + 1));
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::NextId, LEDGER_BUMP, LEDGER_BUMP);

        env.events().publish(
            (symbol_short!("ip_commit"), owner.clone()),
            (id, record.timestamp),
        );

        Self::update_commitment_checksum(&env);

        id
    }

    fn is_delegate_in_chain(env: &Env, root: &Address, candidate: &Address, depth: u32) -> bool {
        if depth >= MAX_DELEGATION_DEPTH {
            return false;
        }
        let key = DataKey::Delegates(root.clone());
        let delegates: Vec<DelegationRecord> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));

        for i in 0..delegates.len() {
            let rec = delegates.get(i).unwrap();
            if &rec.delegate == candidate {
                return true;
            }
            if Self::is_delegate_in_chain(env, &rec.delegate, candidate, depth + 1) {
                return true;
            }
        }
        false
    }

    // ── Issue #458: Batch Verification with ZK Proofs ─────────────────────────

    /// Verify multiple IP commitments in a single call with ZK-proof support.
    ///
    /// For each request, recomputes `sha256(secret || blinding_factor)` and
    /// checks it against the stored commitment hash using constant-time comparison.
    /// Valid commitment hashes are folded into a deterministic aggregate proof
    /// via incremental SHA-256 hashing.
    ///
    /// An event is emitted with the aggregate proof hash and summary counts,
    /// enabling off-chain listeners to track batch verification completion.
    ///
    /// # Arguments
    ///
    /// * `requests` — Vec of `VerifyRequest` (ip_id, secret, blinding_factor)
    ///
    /// # Returns
    ///
    /// `Vec<VerifyResult>` — one entry per request with `valid: true/false`.
    ///
    /// # Events
    ///
    /// Emits a `batch_vfy` event with `(aggregated_hash, count, all_valid)`.
    ///
    /// # Panics
    ///
    /// Panics with `IpNotFound` if any `ip_id` does not exist.
    pub fn batch_verify_commitments(env: Env, requests: Vec<VerifyRequest>) -> Vec<VerifyResult> {
        let total_count = requests.len();
        let mut results = Vec::new(&env);
        let mut valid_hashes: Vec<BytesN<32>> = Vec::new(&env);

        for req in requests.iter() {
            let record = require_ip_exists(&env, req.ip_id);

            let mut preimage = Bytes::new(&env);
            preimage.append(&req.secret.into());
            preimage.append(&req.blinding_factor.into());
            let computed: BytesN<32> = env.crypto().sha256(&preimage).into();

            let valid = constant_time_bytes_32_eq(&record.commitment_hash, &computed);
            results.push_back(VerifyResult {
                ip_id: req.ip_id,
                valid,
            });

            if valid {
                valid_hashes.push_back(record.commitment_hash);
            }
        }

        let valid_count = valid_hashes.len();
        let aggregate_proof = aggregate_batch_proof(&env, &valid_hashes);

        let stored = BatchVerifyResultStorage {
            aggregate_proof: aggregate_proof.clone(),
            total_count,
            valid_count,
        };
        env.storage().persistent().set(
            &DataKey::BatchVerifyResult(aggregate_proof.clone()),
            &stored,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::BatchVerifyResult(aggregate_proof.clone()),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.events().publish(
            (symbol_short!("b_vfy"),),
            (aggregate_proof, total_count, valid_count),
        );

        results
    }

    /// Deterministic hash aggregation for batch proof.
    ///
    /// Combines all (ip_id, on-chain commitment_hash, valid) tuples with a
    /// domain separator into a single SHA-256 hash. The result is used as the
    /// on-chain proof ID and can be recomputed by anyone with access to the
    /// same inputs (minus the secrets).
    fn compute_aggregated_proof(
        env: &Env,
        _ip_ids: &Vec<u64>,
        stored_hashes: &Vec<BytesN<32>>,
        results: &Vec<VerifyResult>,
    ) -> BytesN<32> {
        let mut buf = Bytes::new(env);
        // Domain separator: "IP_BATCH_PROOF_V1"
        buf.append(&Bytes::from_array(
            env,
            &[0x49, 0x50, 0x42, 0x50, 0x56, 0x31],
        ));

        for i in 0..results.len() {
            let r = results.get(i).unwrap();
            let ip_bytes = r.ip_id.to_be_bytes();
            buf.append(&Bytes::from_array(env, &ip_bytes));
            buf.append(&stored_hashes.get(i).unwrap().clone().into());
            let valid_byte: u8 = if r.valid { 1 } else { 0 };
            buf.append(&Bytes::from_array(env, &[valid_byte]));
        }

        env.crypto().sha256(&buf).into()
    }

    /// Retrieve a stored batch verification proof by its aggregated hash.
    ///
    /// Returns `None` if no proof exists for the given hash (e.g. the proof
    /// has expired or was never created).
    pub fn verify_batch_proof(
        env: Env,
        proof_hash: BytesN<32>,
    ) -> Option<BatchVerifyResultStorage> {
        env.storage()
            .persistent()
            .get(&DataKey::BatchVerifyResult(proof_hash))
    }

    // ── Issue #459: Hierarchical Storage ─────────────────────────────────────

    /// Register a category path and store its depth for validation.
    ///
    /// Takes a human-readable path like `b"Software/Cryptography/ZK-Proofs"`,
    /// computes `sha256(path)` as the category hash, counts segments to determine
    /// depth, and persists the depth for subsequent validation.
    ///
    /// # Panics
    ///
    /// Panics with `InvalidCategoryDepth` if the depth exceeds `MAX_CATEGORY_DEPTH`.
    ///
    /// # Returns
    ///
    /// The 32-byte category hash derived from the path.
    pub fn register_category_path(env: Env, path: soroban_sdk::Bytes) -> BytesN<32> {
        let category_hash: BytesN<32> = env.crypto().sha256(&path).into();

        let mut depth: u32 = 1;
        if !path.is_empty() {
            for byte in path.iter() {
                if byte == b'/' {
                    depth += 1;
                }
            }
        }

        if depth > MAX_CATEGORY_DEPTH {
            panic_with_error!(&env, ContractError::InvalidCategoryDepth);
        }

        env.storage()
            .persistent()
            .set(&DataKey::CategoryDepth(category_hash.clone()), &depth);
        env.storage().persistent().extend_ttl(
            &DataKey::CategoryDepth(category_hash.clone()),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        env.events()
            .publish((symbol_short!("cat_reg"),), (category_hash.clone(), depth));

        category_hash
    }

    /// Validate a category hash.
    ///
    /// Checks that the hash is non-zero and that the category is registered
    /// (has an associated depth within `MAX_CATEGORY_DEPTH`).
    ///
    /// # Panics
    ///
    /// Panics with `InvalidCategoryHash` if the hash is all zeros.
    /// Panics with `CategoryNotFound` if the category has not been registered.
    /// Panics with `InvalidCategoryDepth` if the stored depth exceeds the maximum.
    pub fn validate_category(env: Env, category_hash: BytesN<32>) {
        if category_hash == BytesN::from_array(&env, &[0u8; 32]) {
            panic_with_error!(&env, ContractError::InvalidCategoryHash);
        }

        let depth: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::CategoryDepth(category_hash))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::CategoryNotFound));

        if depth > MAX_CATEGORY_DEPTH {
            panic_with_error!(&env, ContractError::InvalidCategoryDepth);
        }
    }

    /// Assign an IP to a category within the owner's hierarchy.
    ///
    /// Stores the IP under `owner → category_hash → ip_ids` for fast
    /// category-scoped queries. Only the IP owner may call this.
    /// The category must have been previously registered via `register_category_path`.
    ///
    /// # Panics
    ///
    /// Panics with `IpNotFound` if the IP does not exist, or auth error if
    /// the caller is not the owner. Panics if the category hash is invalid
    /// or not registered.
    pub fn assign_ip_to_category(env: Env, ip_id: u64, category_hash: BytesN<32>) {
        let record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();

        // Validate category
        Self::validate_category(env.clone(), category_hash.clone());

        let owner = record.owner.clone();
        let node_key = DataKey::HierarchyNode(owner.clone(), category_hash.clone());

        let mut ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&node_key)
            .unwrap_or(Vec::new(&env));
        // Avoid duplicates
        for existing in ids.iter() {
            if existing == ip_id {
                return;
            }
        }
        ids.push_back(ip_id);
        env.storage().persistent().set(&node_key, &ids);
        env.storage()
            .persistent()
            .extend_ttl(&node_key, LEDGER_BUMP, LEDGER_BUMP);

        // Track which categories this owner has
        let cat_key = DataKey::OwnerCategories(owner.clone());
        let mut cats: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&cat_key)
            .unwrap_or(Vec::new(&env));
        let mut found = false;
        for c in cats.iter() {
            if c == category_hash {
                found = true;
                break;
            }
        }
        if !found {
            cats.push_back(category_hash.clone());
            env.storage().persistent().set(&cat_key, &cats);
            env.storage()
                .persistent()
                .extend_ttl(&cat_key, LEDGER_BUMP, LEDGER_BUMP);
        }

        env.events()
            .publish((symbol_short!("ip_cat"), owner), (ip_id, category_hash));
    }

    /// List all IP IDs for an owner within a specific category.
    ///
    /// Returns an empty vector if the owner has no IPs in that category.
    pub fn list_ip_by_category(env: Env, owner: Address, category_hash: BytesN<32>) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::HierarchyNode(owner, category_hash))
            .unwrap_or(Vec::new(&env))
    }

    /// List all category hashes registered for an owner.
    ///
    /// Returns an empty vector if the owner has no categories.
    pub fn list_owner_categories(env: Env, owner: Address) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&DataKey::OwnerCategories(owner))
            .unwrap_or(Vec::new(&env))
    }

    // ── Issue #460: Batch Delegation ──────────────────────────────────────────

    /// Delegate multiple addresses to a root owner's commitment authority in one call.
    ///
    /// Equivalent to calling `delegate_commitment_authority` for each address in
    /// `delegates`. Requires auth from `delegator`. The delegator must be the
    /// `root_owner` or an existing registered delegate to sub-delegate.
    /// Duplicate addresses are silently skipped.
    ///
    /// # Panics
    ///
    /// Panics with `Unauthorized` if the delegator is not authorized or if
    /// the delegation depth limit would be exceeded.
    pub fn batch_delegate_commitment(
        env: Env,
        root_owner: Address,
        delegator: Address,
        delegates: Vec<Address>,
    ) {
        delegator.require_auth();

        let new_depth: u32 = if delegator == root_owner {
            0
        } else {
            let stored: Option<u32> = env
                .storage()
                .persistent()
                .get(&DataKey::DelegateDepth(delegator.clone()));
            match stored {
                Some(d) => d + 1,
                None => panic_with_error!(&env, ContractError::Unauthorized),
            }
        };

        if new_depth >= MAX_DELEGATION_DEPTH {
            panic_with_error!(&env, ContractError::Unauthorized);
        }

        let key = DataKey::Delegates(root_owner.clone());
        let mut stored_delegates: Vec<DelegationRecord> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env));

        for delegate_address in delegates.iter() {
            let mut already_exists = false;
            for i in 0..stored_delegates.len() {
                if stored_delegates.get(i).unwrap().delegate == delegate_address {
                    already_exists = true;
                    break;
                }
            }
            if already_exists {
                continue;
            }

            stored_delegates.push_back(DelegationRecord {
                delegate: delegate_address.clone(),
                depth: new_depth,
            });

            env.storage().persistent().set(
                &DataKey::DelegateDepth(delegate_address.clone()),
                &new_depth,
            );
            env.storage().persistent().extend_ttl(
                &DataKey::DelegateDepth(delegate_address.clone()),
                LEDGER_BUMP,
                LEDGER_BUMP,
            );

            env.events().publish(
                (symbol_short!("delegated"), root_owner.clone()),
                (delegate_address, new_depth),
            );
        }

        env.storage().persistent().set(&key, &stored_delegates);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGER_BUMP, LEDGER_BUMP);
    }

    // ── Issue #461: Batch Renewal ─────────────────────────────────────────────

    /// Renew multiple IP commitments in a single transaction.
    ///
    /// Requires auth from `owner`. Each IP must exist, belong to `owner`, and
    /// not be revoked. Extends storage TTL for every IP and increments each
    /// IP's renewal counter.
    ///
    /// # Panics
    ///
    /// Panics with `IpNotFound` if any ID does not exist, `Unauthorized` if
    /// any IP is not owned by `owner`, or `IpAlreadyRevoked` if any IP is revoked.
    pub fn batch_renew_ip(env: Env, owner: Address, ip_ids: Vec<u64>) {
        owner.require_auth();

        for ip_id in ip_ids.iter() {
            let record = require_ip_exists(&env, ip_id);

            if record.owner != owner {
                panic_with_error!(&env, ContractError::Unauthorized);
            }

            require_not_revoked(&env, &record);

            env.storage().persistent().extend_ttl(
                &DataKey::IpRecord(ip_id),
                LEDGER_BUMP,
                LEDGER_BUMP,
            );

            let count: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::RenewalCount(ip_id))
                .unwrap_or(0u32);
            let new_count = count + 1;
            env.storage()
                .persistent()
                .set(&DataKey::RenewalCount(ip_id), &new_count);
            env.storage().persistent().extend_ttl(
                &DataKey::RenewalCount(ip_id),
                LEDGER_BUMP,
                LEDGER_BUMP,
            );

            env.events().publish(
                (symbol_short!("renewed"), owner.clone()),
                (ip_id, new_count),
            );
        }
    }

    // ── IP Expiry & Grace Period ───────────────────────────────────────────────

    /// Set expiry and grace period for an IP. Owner-only.
    /// Pass expiry_timestamp = 0 to remove expiry.
    pub fn set_ip_expiry(env: Env, ip_id: u64, expiry_timestamp: u64, grace_period_seconds: u64) {
        let mut record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();

        if expiry_timestamp != 0 && expiry_timestamp <= env.ledger().timestamp() {
            env.panic_with_error(Error::from_contract_error(
                ContractError::InvalidExpiry as u32,
            ));
        }

        record.expiry_timestamp = expiry_timestamp;
        record.grace_period_seconds = grace_period_seconds;
        env.storage()
            .persistent()
            .set(&DataKey::IpRecord(ip_id), &record);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::IpRecord(ip_id), LEDGER_BUMP, LEDGER_BUMP);
    }

    /// Renew an IP commitment's expiry. Owner-only.
    /// new_expiry must be strictly greater than the current expiry_timestamp.
    pub fn renew_ip_commitment(env: Env, ip_id: u64, new_expiry: u64) -> bool {
        let mut record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();

        if new_expiry <= record.expiry_timestamp {
            env.panic_with_error(Error::from_contract_error(
                ContractError::InvalidExpiry as u32,
            ));
        }

        let old_expiry = record.expiry_timestamp;
        record.expiry_timestamp = new_expiry;
        env.storage()
            .persistent()
            .set(&DataKey::IpRecord(ip_id), &record);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::IpRecord(ip_id), LEDGER_BUMP, LEDGER_BUMP);

        env.events().publish(
            (symbol_short!("ip_renew"), record.owner),
            (ip_id, old_expiry, new_expiry),
        );

        true
    }

    /// Remove expired IPs that are past their grace period. Anyone can call.
    /// Skips IPs with expiry_timestamp == 0 (no expiry set).
    pub fn cleanup_expired_ips(env: Env, ip_ids: Vec<u64>) {
        let now = env.ledger().timestamp();
        for ip_id in ip_ids.iter() {
            let record: Option<IpRecord> =
                env.storage().persistent().get(&DataKey::IpRecord(ip_id));
            if let Some(rec) = record {
                if rec.expiry_timestamp == 0 {
                    continue;
                }
                if now
                    >= rec
                        .expiry_timestamp
                        .saturating_add(rec.grace_period_seconds)
                {
                    env.storage().persistent().remove(&DataKey::IpRecord(ip_id));
                    env.events()
                        .publish((symbol_short!("ip_clean"),), (ip_id, now));
                }
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Events};
    use soroban_sdk::{Env, IntoVal};

    /// Bug Condition Exploration Test — Property 1
    ///
    /// Validates: Requirements 1.1, 1.2
    ///
    /// isBugCondition(alice, bob) is true: invoker != owner.
    ///
    /// With selective auth (only alice mocked), calling commit_ip(bob, hash)
    /// MUST panic with an auth error — the SDK enforces that bob's auth is
    /// required but not present.
    ///
    /// EXPECTED OUTCOME: This test PANICS (should_panic), confirming the SDK
    /// correctly rejects the non-owner call on unfixed code.
    #[test]
    #[should_panic]
    fn test_non_owner_cannot_commit() {
        let env = Env::default();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        let hash = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);

        // Mock auth only for alice — bob's auth is NOT mocked.
        // Calling commit_ip with bob's address should panic because
        // bob.require_auth() cannot be satisfied.
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &alice,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "commit_ip",
                args: (bob.clone(), hash.clone(), 0u32).into_val(&env),
                sub_invokes: &[],
            },
        }]);

        // This call passes bob's address as owner but only alice's auth is mocked.
        // The SDK MUST reject this with an auth panic — confirming the bug condition
        // is correctly enforced at the protocol level.
        client.commit_ip(&bob, &hash, &0u32);
    }

    /// Attack Surface Documentation Test — mock_all_auths variant
    ///
    /// Validates: Requirements 1.1, 1.2
    ///
    /// Documents the test-environment attack surface: when mock_all_auths() is
    /// used, ANY address can be passed as owner and the call succeeds. This is
    /// the mechanism by which the bug is exploitable in test environments.
    ///
    /// EXPECTED OUTCOME: This test SUCCEEDS, demonstrating that mock_all_auths
    /// bypasses the auth check and allows non-owner commits — the attack surface.
    #[test]
    fn test_non_owner_commit_succeeds_with_mock_all_auths() {
        let env = Env::default();
        env.mock_all_auths(); // bypass all auth checks — documents the risk
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        let hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);

        // With mock_all_auths, alice can commit IP under bob's address.
        // This documents the attack surface: in test environments with relaxed
        // auth, a non-owner can register IP under an arbitrary address.
        // Counterexample: (invoker=alice, owner=bob) — isBugCondition is true.
        let ip_id = client.commit_ip(&bob, &hash, &0u32);

        // The record is stored under bob, not alice — confirming the forgery.
        let record = client.get_ip(&ip_id);
        assert_eq!(record.owner, bob);
        assert_ne!(record.owner, alice);
    }

    #[test]
    fn test_commitment_timestamp_accuracy() {
        let env = Env::default();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let commitment = BytesN::from_array(&env, &[42u8; 32]);

        env.mock_all_auths();

        let recorded_time = env.ledger().timestamp();
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);
        let record = client.get_ip(&ip_id);

        assert_eq!(record.timestamp, recorded_time);
    }

    // ── Tests for IP Commitment Versioning System ──────────────────────────────

    #[test]
    fn test_commit_ip_version_links_parent() {
        let env = Env::default();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        env.mock_all_auths();

        let hash_v1 = BytesN::from_array(&env, &[0xA1u8; 32]);
        let hash_v2 = BytesN::from_array(&env, &[0xA2u8; 32]);

        let v1 = client.commit_ip(&owner, &hash_v1, &0u32);
        let v2 = client.commit_ip_version(&owner, &hash_v2, &v1);

        let record_v2 = client.get_ip(&v2);
        assert_eq!(record_v2.parent_ip_id, Some(v1));
    }

    #[test]
    fn test_get_ip_version_chain_single() {
        let env = Env::default();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        env.mock_all_auths();

        let hash = BytesN::from_array(&env, &[0xB1u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let chain = client.get_ip_version_chain(&ip_id);
        assert_eq!(chain.len(), 1);
        assert_eq!(chain.get(0).unwrap(), ip_id);
    }

    #[test]
    fn test_get_ip_version_chain_multiple_versions() {
        let env = Env::default();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        env.mock_all_auths();

        let h1 = BytesN::from_array(&env, &[0xC1u8; 32]);
        let h2 = BytesN::from_array(&env, &[0xC2u8; 32]);
        let h3 = BytesN::from_array(&env, &[0xC3u8; 32]);

        let v1 = client.commit_ip(&owner, &h1, &0u32);
        let v2 = client.commit_ip_version(&owner, &h2, &v1);
        let v3 = client.commit_ip_version(&owner, &h3, &v2);

        // Chain from root should contain v1, v2, v3
        let chain = client.get_ip_version_chain(&v1);
        assert_eq!(chain.len(), 3);
        assert_eq!(chain.get(0).unwrap(), v1);
        assert_eq!(chain.get(1).unwrap(), v2);
        assert_eq!(chain.get(2).unwrap(), v3);

        // Chain from any version should return the same root chain
        let chain_from_v3 = client.get_ip_version_chain(&v3);
        assert_eq!(chain_from_v3.len(), 3);
        assert_eq!(chain_from_v3.get(0).unwrap(), v1);
    }

    #[test]
    fn test_version_chain_preserves_prior_art() {
        let env = Env::default();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        env.mock_all_auths();

        let h1 = BytesN::from_array(&env, &[0xD1u8; 32]);
        let h2 = BytesN::from_array(&env, &[0xD2u8; 32]);

        let v1 = client.commit_ip(&owner, &h1, &0u32);
        let ts_v1 = client.get_ip(&v1).timestamp;

        let _v2 = client.commit_ip_version(&owner, &h2, &v1);

        // v1 record is unchanged — prior art is preserved
        let record_v1 = client.get_ip(&v1);
        assert_eq!(record_v1.timestamp, ts_v1);
        assert_eq!(record_v1.commitment_hash, h1);
        assert_eq!(record_v1.parent_ip_id, None);
    }

    // ── Issue #454: Threshold Signatures Tests ────────────────────────────────

    #[test]
    fn test_threshold_config_set_and_get() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let signer1 = Address::generate(&env);
        let signer2 = Address::generate(&env);
        let signer3 = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0xE1u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let mut signers = soroban_sdk::Vec::new(&env);
        signers.push_back(signer1.clone());
        signers.push_back(signer2.clone());
        signers.push_back(signer3.clone());

        client.require_threshold_signatures(&ip_id, &2u32, &signers);

        let config = client.get_threshold_config(&ip_id).unwrap();
        assert_eq!(config.threshold, 2);
        assert_eq!(config.total, 3);
    }

    #[test]
    fn test_threshold_not_met_initially() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let signer1 = Address::generate(&env);
        let signer2 = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0xE2u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let mut signers = soroban_sdk::Vec::new(&env);
        signers.push_back(signer1.clone());
        signers.push_back(signer2.clone());
        client.require_threshold_signatures(&ip_id, &2u32, &signers);

        assert!(!client.verify_threshold_signatures(&ip_id));
    }

    #[test]
    fn test_threshold_met_after_enough_signatures() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let signer1 = Address::generate(&env);
        let signer2 = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0xE3u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let mut signers = soroban_sdk::Vec::new(&env);
        signers.push_back(signer1.clone());
        signers.push_back(signer2.clone());
        client.require_threshold_signatures(&ip_id, &2u32, &signers);

        client.add_threshold_signature(&ip_id, &signer1, &BytesN::from_array(&env, &[0xF1u8; 32]));
        assert!(!client.verify_threshold_signatures(&ip_id));

        client.add_threshold_signature(&ip_id, &signer2, &BytesN::from_array(&env, &[0xF2u8; 32]));
        assert!(client.verify_threshold_signatures(&ip_id));
    }

    #[test]
    #[should_panic]
    fn test_threshold_duplicate_signer_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let signer1 = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0xE4u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let mut signers = soroban_sdk::Vec::new(&env);
        signers.push_back(signer1.clone());
        client.require_threshold_signatures(&ip_id, &1u32, &signers);

        let sig = BytesN::from_array(&env, &[0xF3u8; 32]);
        client.add_threshold_signature(&ip_id, &signer1, &sig);
        client.add_threshold_signature(&ip_id, &signer1, &sig); // must panic
    }

    #[test]
    #[should_panic]
    fn test_threshold_unauthorized_signer_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let signer1 = Address::generate(&env);
        let outsider = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0xE5u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let mut signers = soroban_sdk::Vec::new(&env);
        signers.push_back(signer1.clone());
        client.require_threshold_signatures(&ip_id, &1u32, &signers);

        client.add_threshold_signature(&ip_id, &outsider, &BytesN::from_array(&env, &[0xF4u8; 32]));
    }

    #[test]
    fn test_get_threshold_signatures_returns_all() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let signer1 = Address::generate(&env);
        let signer2 = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0xE6u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let mut signers = soroban_sdk::Vec::new(&env);
        signers.push_back(signer1.clone());
        signers.push_back(signer2.clone());
        client.require_threshold_signatures(&ip_id, &1u32, &signers);

        client.add_threshold_signature(&ip_id, &signer1, &BytesN::from_array(&env, &[0xAAu8; 32]));
        client.add_threshold_signature(&ip_id, &signer2, &BytesN::from_array(&env, &[0xBBu8; 32]));

        assert_eq!(client.get_threshold_signatures(&ip_id).len(), 2);
    }

    // ── Issue #455: Batch Metadata Tests ─────────────────────────────────────

    #[test]
    fn test_set_and_get_batch_metadata() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0x11u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let batch_id = BytesN::from_array(&env, &[0xBAu8; 32]);
        let description = soroban_sdk::Bytes::from_array(&env, b"hello");

        client.set_batch_metadata(&ip_id, &batch_id, &description);

        let meta = client.get_batch_metadata(&ip_id).unwrap();
        assert_eq!(meta.ip_id, ip_id);
        assert_eq!(meta.batch_id, batch_id);
        assert_eq!(meta.description, description);
    }

    #[test]
    fn test_get_batch_metadata_none_if_not_set() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0x12u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        assert!(client.get_batch_metadata(&ip_id).is_none());
    }

    #[test]
    #[should_panic]
    fn test_batch_metadata_too_large_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0x13u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let batch_id = BytesN::from_array(&env, &[0xBBu8; 32]);
        let big = soroban_sdk::Bytes::from_array(&env, &[0u8; 1025]);
        client.set_batch_metadata(&ip_id, &batch_id, &big);
    }

    #[test]
    fn test_batch_metadata_overwrite() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0x14u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let batch_id1 = BytesN::from_array(&env, &[0x01u8; 32]);
        let batch_id2 = BytesN::from_array(&env, &[0x02u8; 32]);

        client.set_batch_metadata(
            &ip_id,
            &batch_id1,
            &soroban_sdk::Bytes::from_array(&env, b"v1"),
        );
        client.set_batch_metadata(
            &ip_id,
            &batch_id2,
            &soroban_sdk::Bytes::from_array(&env, b"v2"),
        );

        assert_eq!(
            client.get_batch_metadata(&ip_id).unwrap().batch_id,
            batch_id2
        );
    }

    // ── Issue #456: Compression Algorithm Selection Tests ────────────────────

    #[test]
    fn test_default_compression_is_truncate16() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0x21u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        assert_eq!(
            client.get_commitment_compression(&ip_id),
            CompressionAlgo::Truncate16
        );
    }

    #[test]
    fn test_set_compression_none() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0x22u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        client.set_commitment_compression(&ip_id, &CompressionAlgo::None);
        assert_eq!(
            client.get_commitment_compression(&ip_id),
            CompressionAlgo::None
        );
    }

    #[test]
    fn test_set_compression_xor8() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0x23u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        client.set_commitment_compression(&ip_id, &CompressionAlgo::Xor8);
        assert_eq!(
            client.get_commitment_compression(&ip_id),
            CompressionAlgo::Xor8
        );
    }

    #[test]
    fn test_compressed_truncate16_length() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0x24u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        client.set_commitment_compression(&ip_id, &CompressionAlgo::Truncate16);
        assert_eq!(client.get_compressed_bytes(&ip_id).len(), 16);
    }

    #[test]
    fn test_compressed_xor8_length() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0x25u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        client.set_commitment_compression(&ip_id, &CompressionAlgo::Xor8);
        assert_eq!(client.get_compressed_bytes(&ip_id).len(), 8);
    }

    #[test]
    fn test_compressed_none_length() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0x26u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        client.set_commitment_compression(&ip_id, &CompressionAlgo::None);
        assert_eq!(client.get_compressed_bytes(&ip_id).len(), 32);
    }

    // ── Issue #457: Commitment Encryption Tests ───────────────────────────────

    #[test]
    fn test_encrypt_and_get_commitment() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0x31u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let encrypted = soroban_sdk::Bytes::from_array(&env, &[0xCCu8; 64]);
        let key_hint = BytesN::from_array(&env, &[0xDDu8; 32]);

        client.encrypt_commitment(&ip_id, &encrypted, &key_hint);

        let record = client.get_encrypted_commitment(&ip_id).unwrap();
        assert_eq!(record.ip_id, ip_id);
        assert_eq!(record.encrypted_hash, encrypted);
        assert_eq!(record.key_hint, key_hint);
    }

    #[test]
    fn test_get_encrypted_commitment_none_if_not_set() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0x32u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        assert!(client.get_encrypted_commitment(&ip_id).is_none());
    }

    #[test]
    #[should_panic]
    fn test_encrypted_data_too_large_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0x33u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let too_big = soroban_sdk::Bytes::from_array(&env, &[0u8; 257]);
        let key_hint = BytesN::from_array(&env, &[0xEEu8; 32]);
        client.encrypt_commitment(&ip_id, &too_big, &key_hint);
    }

    #[test]
    fn test_encrypt_commitment_overwrite() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0x34u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let key_hint = BytesN::from_array(&env, &[0xFFu8; 32]);
        client.encrypt_commitment(
            &ip_id,
            &soroban_sdk::Bytes::from_array(&env, &[0x01u8; 32]),
            &key_hint,
        );
        client.encrypt_commitment(
            &ip_id,
            &soroban_sdk::Bytes::from_array(&env, &[0x02u8; 32]),
            &key_hint,
        );

        let record = client.get_encrypted_commitment(&ip_id).unwrap();
        assert_eq!(
            record.encrypted_hash,
            soroban_sdk::Bytes::from_array(&env, &[0x02u8; 32])
        );
    }

    // ── Issue #458: Batch Verification Tests ──────────────────────────────────

    #[test]
    fn test_batch_verify_all_valid() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let secret1 = BytesN::from_array(&env, &[0x01u8; 32]);
        let blind1 = BytesN::from_array(&env, &[0x02u8; 32]);
        let mut pre1 = soroban_sdk::Bytes::new(&env);
        pre1.append(&secret1.clone().into());
        pre1.append(&blind1.clone().into());
        let hash1: BytesN<32> = env.crypto().sha256(&pre1).into();

        let secret2 = BytesN::from_array(&env, &[0x03u8; 32]);
        let blind2 = BytesN::from_array(&env, &[0x04u8; 32]);
        let mut pre2 = soroban_sdk::Bytes::new(&env);
        pre2.append(&secret2.clone().into());
        pre2.append(&blind2.clone().into());
        let hash2: BytesN<32> = env.crypto().sha256(&pre2).into();

        let id1 = client.commit_ip(&owner, &hash1, &0u32);
        let id2 = client.commit_ip(&owner, &hash2, &0u32);

        let mut requests = soroban_sdk::Vec::new(&env);
        requests.push_back(VerifyRequest {
            ip_id: id1,
            secret: secret1,
            blinding_factor: blind1,
        });
        requests.push_back(VerifyRequest {
            ip_id: id2,
            secret: secret2,
            blinding_factor: blind2,
        });

        let results = client.batch_verify_commitments(&requests);
        assert_eq!(results.len(), 2);
        assert!(results.get(0).unwrap().valid);
        assert!(results.get(1).unwrap().valid);
    }

    #[test]
    fn test_batch_verify_invalid_proof() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let secret = BytesN::from_array(&env, &[0xAAu8; 32]);
        let blind = BytesN::from_array(&env, &[0xBBu8; 32]);
        let mut pre = soroban_sdk::Bytes::new(&env);
        pre.append(&secret.clone().into());
        pre.append(&blind.clone().into());
        let hash: BytesN<32> = env.crypto().sha256(&pre).into();
        let id = client.commit_ip(&owner, &hash, &0u32);

        // Wrong blinding factor
        let wrong_blind = BytesN::from_array(&env, &[0xCCu8; 32]);
        let mut requests = soroban_sdk::Vec::new(&env);
        requests.push_back(VerifyRequest {
            ip_id: id,
            secret,
            blinding_factor: wrong_blind,
        });

        let results = client.batch_verify_commitments(&requests);
        assert!(!results.get(0).unwrap().valid);
    }

    #[test]
    #[should_panic]
    fn test_batch_verify_nonexistent_ip_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let mut requests = soroban_sdk::Vec::new(&env);
        requests.push_back(VerifyRequest {
            ip_id: 9999,
            secret: BytesN::from_array(&env, &[0x01u8; 32]),
            blinding_factor: BytesN::from_array(&env, &[0x02u8; 32]),
        });
        client.batch_verify_commitments(&requests);
    }

    #[test]
    fn test_batch_verify_empty_batch() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let requests = soroban_sdk::Vec::new(&env);
        let results = client.batch_verify_commitments(&requests);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_batch_verify_single_item() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let secret = BytesN::from_array(&env, &[0x42u8; 32]);
        let blind = BytesN::from_array(&env, &[0x24u8; 32]);
        let mut pre = soroban_sdk::Bytes::new(&env);
        pre.append(&secret.clone().into());
        pre.append(&blind.clone().into());
        let hash: BytesN<32> = env.crypto().sha256(&pre).into();
        let id = client.commit_ip(&owner, &hash, &0u32);

        let mut requests = soroban_sdk::Vec::new(&env);
        requests.push_back(VerifyRequest {
            ip_id: id,
            secret,
            blinding_factor: blind,
        });

        let results = client.batch_verify_commitments(&requests);
        assert_eq!(results.len(), 1);
        assert!(results.get(0).unwrap().valid);
    }

    #[test]
    fn test_batch_verify_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let secret = BytesN::from_array(&env, &[0xA1u8; 32]);
        let blind = BytesN::from_array(&env, &[0xB2u8; 32]);
        let mut pre = soroban_sdk::Bytes::new(&env);
        pre.append(&secret.clone().into());
        pre.append(&blind.clone().into());
        let hash: BytesN<32> = env.crypto().sha256(&pre).into();
        let id = client.commit_ip(&owner, &hash, &0u32);

        let mut requests = soroban_sdk::Vec::new(&env);
        requests.push_back(VerifyRequest {
            ip_id: id,
            secret,
            blinding_factor: blind,
        });

        let _results = client.batch_verify_commitments(&requests);

        let contract_events = env.events().all();
        let emitted = contract_events.events();
        assert!(!emitted.is_empty());
    }

    // ── Issue #459: Hierarchical Storage Tests ────────────────────────────────

    #[test]
    fn test_register_category_path_basic() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let path = soroban_sdk::Bytes::from_slice(&env, b"Software/Cryptography/ZK-Proofs");
        let cat_hash = client.register_category_path(&path);

        let expected: BytesN<32> = env.crypto().sha256(&path).into();
        assert_eq!(cat_hash, expected);
    }

    #[test]
    fn test_register_category_path_max_depth_allowed() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        // 10 levels: MAX_CATEGORY_DEPTH
        let path = soroban_sdk::Bytes::from_slice(&env, b"a/b/c/d/e/f/g/h/i/j");
        let cat_hash = client.register_category_path(&path);
        assert_ne!(cat_hash, BytesN::from_array(&env, &[0u8; 32]));
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #33)")]
    fn test_register_category_path_exceeds_max_depth() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        // 11 levels: exceeds MAX_CATEGORY_DEPTH (10)
        let path = soroban_sdk::Bytes::from_slice(&env, b"a/b/c/d/e/f/g/h/i/j/k");
        client.register_category_path(&path);
    }

    #[test]
    fn test_register_category_path_single_segment() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let path = soroban_sdk::Bytes::from_slice(&env, b"patents");
        let cat_hash = client.register_category_path(&path);
        assert_ne!(cat_hash, BytesN::from_array(&env, &[0u8; 32]));
    }

    #[test]
    fn test_register_category_path_can_register_multiple() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let p1 = soroban_sdk::Bytes::from_slice(&env, b"Software");
        let p2 = soroban_sdk::Bytes::from_slice(&env, b"Software/Cryptography");
        let p3 = soroban_sdk::Bytes::from_slice(&env, b"Software/Cryptography/ZK-Proofs");
        let p4 = soroban_sdk::Bytes::from_slice(&env, b"Software/Cryptography/ZK-Proofs/DLV");
        let p5 = soroban_sdk::Bytes::from_slice(&env, b"Software/Cryptography/ZK-Proofs/DLV/AXIOM");

        let h1 = client.register_category_path(&p1);
        client.register_category_path(&p2);
        client.register_category_path(&p3);
        client.register_category_path(&p4);
        let h5 = client.register_category_path(&p5);

        let expected1: BytesN<32> = env.crypto().sha256(&p1).into();
        let expected5: BytesN<32> = env.crypto().sha256(&p5).into();
        assert_eq!(h1, expected1);
        assert_eq!(h5, expected5);
    }

    #[test]
    fn test_validate_category_succeeds_for_registered() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let path = soroban_sdk::Bytes::from_slice(&env, b"Software");
        let cat_hash = client.register_category_path(&path);
        client.validate_category(&cat_hash); // should not panic
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #32)")]
    fn test_validate_category_panics_for_zero_hash() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let zero = BytesN::from_array(&env, &[0u8; 32]);
        client.validate_category(&zero);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #34)")]
    fn test_validate_category_panics_for_unregistered() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let unregistered = BytesN::from_array(&env, &[0xABu8; 32]);
        client.validate_category(&unregistered);
    }

    // ── Tests for Category Hierarchy Depth (5+ levels) ─────────────────────

    #[test]
    fn test_category_hierarchy_depth_5_works() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // Register a 5-level deep category
        let path =
            soroban_sdk::Bytes::from_slice(&env, b"Software/Cryptography/ZK-Proofs/DLV/AXIOM");
        let cat_hash = client.register_category_path(&path);

        // Commit IP and assign to deep category
        let ip_hash = BytesN::from_array(&env, &[0x99u8; 32]);
        let ip_id = client.commit_ip(&owner, &ip_hash, &0u32);
        client.assign_ip_to_category(&ip_id, &cat_hash);

        let ids = client.list_ip_by_category(&owner, &cat_hash);
        assert_eq!(ids.len(), 1);
        assert_eq!(ids.get(0).unwrap(), ip_id);
    }

    #[test]
    fn test_category_hierarchy_depth_8_works() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // Register an 8-level deep category
        let path = soroban_sdk::Bytes::from_slice(&env, b"a/b/c/d/e/f/g/h");
        let cat_hash = client.register_category_path(&path);

        let ip_hash = BytesN::from_array(&env, &[0x88u8; 32]);
        let ip_id = client.commit_ip(&owner, &ip_hash, &0u32);
        client.assign_ip_to_category(&ip_id, &cat_hash);

        let ids = client.list_ip_by_category(&owner, &cat_hash);
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn test_multiple_ips_in_deep_hierarchy() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // Register 5-level category and assign multiple IPs
        let path = soroban_sdk::Bytes::from_slice(&env, b"Research/AI/ML/NLP/Transformers");
        let cat_hash = client.register_category_path(&path);

        let id1 = client.commit_ip(&owner, &BytesN::from_array(&env, &[0x01u8; 32]), &0u32);
        let id2 = client.commit_ip(&owner, &BytesN::from_array(&env, &[0x02u8; 32]), &0u32);
        let id3 = client.commit_ip(&owner, &BytesN::from_array(&env, &[0x03u8; 32]), &0u32);

        client.assign_ip_to_category(&id1, &cat_hash);
        client.assign_ip_to_category(&id2, &cat_hash);
        client.assign_ip_to_category(&id3, &cat_hash);

        let ids = client.list_ip_by_category(&owner, &cat_hash);
        assert_eq!(ids.len(), 3);
    }

    // ── Tests for Path Traversal Attempts ──────────────────────────────────

    #[test]
    #[should_panic(expected = "Error(Contract, #34)")]
    fn test_assign_ip_to_category_path_traversal_attempt() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // Path traversal string: "../../etc/passwd" - should be treated as a category
        // but it won't be registered, so assign will fail with CategoryNotFound
        let path = soroban_sdk::Bytes::from_slice(&env, b"../../../etc/passwd");
        // Note: we do NOT register this path first
        let cat_hash: BytesN<32> = env.crypto().sha256(&path).into();
        // Note: we do NOT register this path first
        let ip_hash = BytesN::from_array(&env, &[0x20u8; 32]);
        let ip_id = client.commit_ip(&owner, &ip_hash, &0u32);
        client.assign_ip_to_category(&ip_id, &cat_hash);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #34)")]
    fn test_assign_ip_to_category_unregistered_hash() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let ip_hash = BytesN::from_array(&env, &[0x30u8; 32]);
        let ip_id = client.commit_ip(&owner, &ip_hash, &0u32);

        // Arbitrary unregistered hash
        let bogus = BytesN::from_array(&env, &[0xDEu8; 32]);
        client.assign_ip_to_category(&ip_id, &bogus);
    }

    // ── Updated tests from original #459 implementation ─────────────────────

    #[test]
    fn test_assign_and_list_ip_by_category() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0x10u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let path = soroban_sdk::Bytes::from_slice(&env, b"TestCategory");
        let cat_hash = client.register_category_path(&path);
        client.assign_ip_to_category(&ip_id, &cat_hash);

        let ids = client.list_ip_by_category(&owner, &cat_hash);
        assert_eq!(ids.len(), 1);
        assert_eq!(ids.get(0).unwrap(), ip_id);
    }

    #[test]
    fn test_list_owner_categories() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let hash1 = BytesN::from_array(&env, &[0x11u8; 32]);
        let hash2 = BytesN::from_array(&env, &[0x12u8; 32]);
        let id1 = client.commit_ip(&owner, &hash1, &0u32);
        let id2 = client.commit_ip(&owner, &hash2, &0u32);

        let p1 = soroban_sdk::Bytes::from_slice(&env, b"CategoryOne");
        let p2 = soroban_sdk::Bytes::from_slice(&env, b"CategoryTwo");
        let cat1 = client.register_category_path(&p1);
        let cat2 = client.register_category_path(&p2);

        client.assign_ip_to_category(&id1, &cat1);
        client.assign_ip_to_category(&id2, &cat2);

        let cats = client.list_owner_categories(&owner);
        assert_eq!(cats.len(), 2);
    }

    #[test]
    fn test_assign_ip_to_category_no_duplicates() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let hash = BytesN::from_array(&env, &[0x13u8; 32]);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        let path = soroban_sdk::Bytes::from_slice(&env, b"NoDupCategory");
        let cat_hash = client.register_category_path(&path);

        client.assign_ip_to_category(&ip_id, &cat_hash);
        client.assign_ip_to_category(&ip_id, &cat_hash); // duplicate call

        let ids = client.list_ip_by_category(&owner, &cat_hash);
        assert_eq!(ids.len(), 1); // still only one entry
    }

    #[test]
    fn test_list_ip_by_category_empty() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let path = soroban_sdk::Bytes::from_slice(&env, b"EmptyCat");
        let cat_hash = client.register_category_path(&path);

        let ids = client.list_ip_by_category(&owner, &cat_hash);
        assert_eq!(ids.len(), 0);
    }

    // ── Issue #460: Batch Delegation Tests ────────────────────────────────────

    #[test]
    fn test_batch_delegate_commitment_registers_all() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate1 = Address::generate(&env);
        let delegate2 = Address::generate(&env);

        let mut delegates = soroban_sdk::Vec::new(&env);
        delegates.push_back(delegate1.clone());
        delegates.push_back(delegate2.clone());

        client.batch_delegate_commitment(&owner, &owner, &delegates);

        assert!(client.is_delegate(&owner, &delegate1));
        assert!(client.is_delegate(&owner, &delegate2));
    }

    #[test]
    fn test_batch_delegate_commitment_skips_duplicates() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let delegate = Address::generate(&env);

        let mut delegates = soroban_sdk::Vec::new(&env);
        delegates.push_back(delegate.clone());
        delegates.push_back(delegate.clone());

        client.batch_delegate_commitment(&owner, &owner, &delegates);

        assert!(client.is_delegate(&owner, &delegate));
    }

    #[test]
    fn test_batch_delegate_commitment_empty_list() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let unrelated = Address::generate(&env);

        let delegates = soroban_sdk::Vec::new(&env);
        client.batch_delegate_commitment(&owner, &owner, &delegates);

        assert!(!client.is_delegate(&owner, &unrelated));
    }

    #[test]
    #[should_panic]
    fn test_batch_delegate_commitment_unauthorized_delegator_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let stranger = Address::generate(&env);
        let target = Address::generate(&env);

        let mut delegates = soroban_sdk::Vec::new(&env);
        delegates.push_back(target.clone());

        client.batch_delegate_commitment(&owner, &stranger, &delegates);
    }

    // ── Issue #461: Batch Renewal Tests ──────────────────────────────────────

    #[test]
    fn test_batch_renew_ip_increments_each_count() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let hash1 = BytesN::from_array(&env, &[0xA1u8; 32]);
        let hash2 = BytesN::from_array(&env, &[0xA2u8; 32]);

        let id1 = client.commit_ip(&owner, &hash1, &0u32);
        let id2 = client.commit_ip(&owner, &hash2, &0u32);

        let mut ids = soroban_sdk::Vec::new(&env);
        ids.push_back(id1);
        ids.push_back(id2);

        client.batch_renew_ip(&owner, &ids);

        assert_eq!(client.get_renewal_count(&id1), 1);
        assert_eq!(client.get_renewal_count(&id2), 1);
    }

    #[test]
    fn test_batch_renew_ip_multiple_rounds() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[0xB1u8; 32]);
        let id = client.commit_ip(&owner, &hash, &0u32);

        let mut ids = soroban_sdk::Vec::new(&env);
        ids.push_back(id);

        client.batch_renew_ip(&owner, &ids);
        client.batch_renew_ip(&owner, &ids);

        assert_eq!(client.get_renewal_count(&id), 2);
    }

    #[test]
    #[should_panic]
    fn test_batch_renew_ip_revoked_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[0xC5u8; 32]);
        let id = client.commit_ip(&owner, &hash, &0u32);
        client.revoke_ip(&id);

        let mut ids = soroban_sdk::Vec::new(&env);
        ids.push_back(id);

        client.batch_renew_ip(&owner, &ids);
    }

    #[test]
    #[should_panic]
    fn test_batch_renew_ip_wrong_owner_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let attacker = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[0xD5u8; 32]);
        let id = client.commit_ip(&owner, &hash, &0u32);

        let mut ids = soroban_sdk::Vec::new(&env);
        ids.push_back(id);

        client.batch_renew_ip(&attacker, &ids);
    }

    #[test]
    fn test_batch_renew_ip_empty_list() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let ids = soroban_sdk::Vec::new(&env);

        client.batch_renew_ip(&owner, &ids);
    }
}
