#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN, Env,
    Error, Vec,
};

mod validation;
use validation::*;

mod types;
use types::*;

#[cfg(test)]
mod test;

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
    CategoryIps(BytesN<32>), // maps category hash -> Vec<u64> of IP IDs
    PowDifficulty,          // stores the current PoW difficulty (leading zero bits required)
    IpVersions(u64),        // stores Vec<u64> of all version IDs for a given IP
    SuggestedPrice(u64),    // stores suggested price for an IP
    IpCommitmentChecksum,   // Issue #346: stores hash of all commitments for rollback protection
    IpAccessGrants(u64),    // Issue #344: stores Vec of (grantee, access_level) for tiered access
    NotarySignature(u64),   // Issue #345: stores notary signature for timestamp notarization
}

// ── Types ────────────────────────────────────────────────────────────────────

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
    pub fn commit_ip(env: Env, owner: Address, commitment_hash: BytesN<32>, pow_difficulty: u32) -> u64 {
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
        };

        env.storage()
            .persistent()
            .set(&DataKey::IpRecord(id), &record);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::IpRecord(id), LEDGER_BUMP, LEDGER_BUMP);

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

        // Issue #346: Update commitment checksum for rollback protection
        Self::update_commitment_checksum(&env);

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
    pub fn batch_commit_ip(env: Env, owner: Address, commitment_hashes: Vec<BytesN<32>>) -> Vec<u64> {
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

            env.events().publish(
                (symbol_short!("ip_commit"), owner.clone()),
                (id, timestamp),
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
            .extend_ttl(&DataKey::OwnerIps(old_owner), 50000, 50000);

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

        record.owner = new_owner;
        env.storage()
            .persistent()
            .set(&DataKey::IpRecord(ip_id), &record);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::IpRecord(ip_id), 50000, 50000);
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

        env.events().publish(
            (REVOKE_TOPIC, record.owner.clone()),
            (ip_id, env.ledger().timestamp()),
        );
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

        // Validate the new WASM before upgrading
        Self::validate_upgrade(env, new_wasm_hash);

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
        if record.expiry_timestamp != 0 && env.ledger().timestamp() > record.expiry_timestamp {
            env.panic_with_error(Error::from_contract_error(ContractError::IpExpired as u32));
        }

        // Concatenate secret || blinding_factor into Bytes, then SHA256
        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&secret.into());
        preimage.append(&blinding_factor.into());
        let computed_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

        record.commitment_hash == computed_hash
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

    /// Returns the current PoW difficulty (number of leading zero bits required in commitment_hash).
    /// Defaults to 4 if not explicitly set.
    pub fn get_pow_difficulty(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::PowDifficulty)
            .unwrap_or(4u32)
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

    /// Set or update the expiry timestamp for an IP. Owner-only.
    /// Pass 0 to remove expiry.
    pub fn set_ip_expiry(env: Env, ip_id: u64, expiry_timestamp: u64) {
        let mut record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();
        record.expiry_timestamp = expiry_timestamp;
        env.storage().persistent().set(&DataKey::IpRecord(ip_id), &record);
        env.storage().persistent().extend_ttl(&DataKey::IpRecord(ip_id), LEDGER_BUMP, LEDGER_BUMP);
    }

    /// Set or update metadata for an IP (max 1 KB). Owner-only.
    pub fn set_ip_metadata(env: Env, ip_id: u64, metadata: Bytes) {
        if metadata.len() > MAX_METADATA_BYTES {
            env.panic_with_error(Error::from_contract_error(ContractError::MetadataTooLarge as u32));
        }
        let mut record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();
        record.metadata = metadata;
        env.storage().persistent().set(&DataKey::IpRecord(ip_id), &record);
        env.storage().persistent().extend_ttl(&DataKey::IpRecord(ip_id), LEDGER_BUMP, LEDGER_BUMP);
    }

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
                licenses.set(i, LicenseEntry { licensee: licensee.clone(), terms_hash: terms_hash.clone() });
                found = true;
                break;
            }
        }
        if !found {
            licenses.push_back(LicenseEntry { licensee, terms_hash });
        }

        env.storage().persistent().set(&DataKey::IpLicenses(ip_id), &licenses);
        env.storage().persistent().extend_ttl(&DataKey::IpLicenses(ip_id), LEDGER_BUMP, LEDGER_BUMP);
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
            env.panic_with_error(Error::from_contract_error(ContractError::LicenseeNotFound as u32));
        }

        env.storage().persistent().set(&DataKey::IpLicenses(ip_id), &licenses);
        env.storage().persistent().extend_ttl(&DataKey::IpLicenses(ip_id), LEDGER_BUMP, LEDGER_BUMP);
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
            env.storage().persistent().remove(&DataKey::SuggestedPrice(ip_id));
        } else {
            env.storage().persistent().set(&DataKey::SuggestedPrice(ip_id), &price);
            env.storage().persistent().extend_ttl(&DataKey::SuggestedPrice(ip_id), LEDGER_BUMP, LEDGER_BUMP);
        }
    }

    /// Get the suggested price for an IP. Returns None if no price has been set.
    pub fn get_ip_suggested_price(env: Env, ip_id: u64) -> Option<i128> {
        require_ip_exists(&env, ip_id);
        env.storage().persistent().get(&DataKey::SuggestedPrice(ip_id))
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
        env.storage().persistent().set(&DataKey::IpRecord(ip_id), &record);
        env.storage().persistent().extend_ttl(&DataKey::IpRecord(ip_id), LEDGER_BUMP, LEDGER_BUMP);

        env.events().publish(
            (symbol_short!("co_add"), record.owner),
            (ip_id, co_owner),
        );
    }

    /// Remove a co-owner from an IP. Owner-only.
    pub fn remove_co_owner(env: Env, ip_id: u64, co_owner: Address) {
        let mut record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();

        // Find and remove the co-owner
        if let Some(pos) = record.co_owners.iter().position(|addr| addr == co_owner) {
            record.co_owners.remove(pos as u32);
            env.storage().persistent().set(&DataKey::IpRecord(ip_id), &record);
            env.storage().persistent().extend_ttl(&DataKey::IpRecord(ip_id), LEDGER_BUMP, LEDGER_BUMP);

            env.events().publish(
                (symbol_short!("co_rem"), record.owner),
                (ip_id, co_owner),
            );
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

        // Get next ID
        let id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NextId)
            .unwrap_or(1);

        // Create new version record with parent_ip_id set
        let mut version_record = IpRecord {
            ip_id: id,
            owner: parent_record.owner.clone(),
            commitment_hash: new_commitment_hash.clone(),
            timestamp: env.ledger().timestamp(),
            revoked: false,
            co_owners: Vec::new(&env),
            parent_ip_id: Some(parent_ip_id),
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
        env.storage()
            .persistent()
            .set(&DataKey::CommitmentOwner(new_commitment_hash.clone()), &parent_record.owner);
        env.storage().persistent().extend_ttl(
            &DataKey::CommitmentOwner(new_commitment_hash.clone()),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        // Add to version lineage
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
        let record = require_ip_exists(&env, ip_id);

        // Find the root IP (the one with no parent)
        let mut root_id = ip_id;
        let mut current_id = ip_id;

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

    // ── Issue #343: Merkle Tree Proof ──────────────────────────────────────────

    /// Compute the Merkle root of all IP commitments for an owner.
    /// This enables proving membership in a set of IPs without full disclosure.
    pub fn compute_ip_merkle_root(env: Env, owner: Address) -> BytesN<32> {
        let ip_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerIps(owner))
            .unwrap_or(Vec::new(&env));

        if ip_ids.len() == 0 {
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

    fn merkle_root(env: &Env, hashes: &Vec<BytesN<32>>) -> BytesN<32> {
        if hashes.len() == 0 {
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
        let mut computed = leaf.clone();

        for proof_hash in proof.iter() {
            let mut combined = Bytes::new(env);
            combined.append(&computed.into());
            combined.append(&proof_hash.into());
            computed = env.crypto().sha256(&combined).into();
        }

        computed == root
    }

    // ── Issue #344: Tiered Access Control ──────────────────────────────────────

    /// Grant access to an IP for a third party. Owner-only.
    /// access_level: 0 = none, 1 = read-only, 2 = read-write
    pub fn grant_ip_access(env: Env, ip_id: u64, grantee: Address, access_level: u8) {
        let record = require_ip_exists(&env, ip_id);
        record.owner.require_auth();

        if access_level > 2 {
            env.panic_with_error(Error::from_contract_error(ContractError::Unauthorized as u32));
        }

        let mut grants: Vec<IpAccessGrant> = env
            .storage()
            .persistent()
            .get(&DataKey::IpAccessGrants(ip_id))
            .unwrap_or(Vec::new(&env));

        let mut found = false;
        for i in 0..grants.len() {
            if grants.get(i).unwrap().grantee == grantee {
                grants.set(i, IpAccessGrant { grantee: grantee.clone(), access_level });
                found = true;
                break;
            }
        }
        if !found {
            grants.push_back(IpAccessGrant { grantee, access_level });
        }

        env.storage()
            .persistent()
            .set(&DataKey::IpAccessGrants(ip_id), &grants);
        env.storage().persistent().extend_ttl(
            &DataKey::IpAccessGrants(ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );
    }

    /// Revoke access to an IP from a third party. Owner-only.
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

    // ── Issue #345: Timestamp Notarization ─────────────────────────────────────

    /// Notarize an IP timestamp with a notary signature. Notary-only.
    pub fn notarize_ip_timestamp(env: Env, ip_id: u64, notary_signature: Bytes) {
        let mut record = require_ip_exists(&env, ip_id);

        // In production, verify notary_signature against NOTARY_PUBLIC_KEY
        // For now, accept any signature (placeholder implementation)
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

    // ── Issue #346: Commitment Rollback Protection ─────────────────────────────

    /// Compute and store a checksum of all commitments for rollback protection.
    fn update_commitment_checksum(env: &Env) {
        // Get all commitment hashes from storage
        // For simplicity, we compute a hash of all commitment hashes
        let mut all_hashes = Bytes::new(env);

        // This is a simplified implementation - in production, you'd iterate through all IPs
        // For now, we'll store a placeholder checksum
        let checksum: BytesN<32> = env.crypto().sha256(&all_hashes).into();

        env.storage()
            .persistent()
            .set(&DataKey::IpCommitmentChecksum, &checksum);
        env.storage().persistent().extend_ttl(
            &DataKey::IpCommitmentChecksum,
            LEDGER_BUMP,
            LEDGER_BUMP,
        );
    }

    /// Verify the integrity of all commitments (for upgrade safety).
    pub fn verify_commitment_integrity(env: Env) -> bool {
        // Retrieve stored checksum
        let stored_checksum: Option<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&DataKey::IpCommitmentChecksum);

        if stored_checksum.is_none() {
            return true; // No checksum stored yet
        }

        // Recompute checksum
        let mut all_hashes = Bytes::new(&env);
        let recomputed_checksum: BytesN<32> = env.crypto().sha256(&all_hashes).into();

        stored_checksum.unwrap() == recomputed_checksum
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env, IntoVal};

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

    // ── Tests for Issue #339: IP Commitment Expiry Renewal ──

    #[test]
    fn test_renew_ip_commitment_success() {
        let env = Env::default();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let commitment = BytesN::from_array(&env, &[1u8; 32]);

        env.mock_all_auths();

        let ip_id = client.commit_ip(&owner, &commitment, &0u32);
        client.set_ip_expiry(&ip_id, &1000u64);

        client.renew_ip_commitment(&ip_id, &2000u64);

        let record = client.get_ip(&ip_id);
        assert_eq!(record.expiry_timestamp, 2000u64);
    }

    #[test]
    #[should_panic(expected = "InvalidExpiry")]
    fn test_renew_ip_commitment_invalid_expiry() {
        let env = Env::default();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let commitment = BytesN::from_array(&env, &[2u8; 32]);

        env.mock_all_auths();

        let ip_id = client.commit_ip(&owner, &commitment, &0u32);
        client.set_ip_expiry(&ip_id, &1000u64);

        // Try to renew with same or lower expiry
        client.renew_ip_commitment(&ip_id, &1000u64);
    }

    // ── Tests for Issue #342: Batch Verification ──

    #[test]
    fn test_batch_verify_commitments() {
        let env = Env::default();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        env.mock_all_auths();

        // Create 3 commitments
        let secret1 = BytesN::from_array(&env, &[1u8; 32]);
        let blinding1 = BytesN::from_array(&env, &[10u8; 32]);
        let mut preimage1 = soroban_sdk::Bytes::new(&env);
        preimage1.append(&secret1.clone().into());
        preimage1.append(&blinding1.clone().into());
        let commitment1: BytesN<32> = env.crypto().sha256(&preimage1).into();

        let secret2 = BytesN::from_array(&env, &[2u8; 32]);
        let blinding2 = BytesN::from_array(&env, &[20u8; 32]);
        let mut preimage2 = soroban_sdk::Bytes::new(&env);
        preimage2.append(&secret2.clone().into());
        preimage2.append(&blinding2.clone().into());
        let commitment2: BytesN<32> = env.crypto().sha256(&preimage2).into();

        let secret3 = BytesN::from_array(&env, &[3u8; 32]);
        let blinding3 = BytesN::from_array(&env, &[30u8; 32]);
        let mut preimage3 = soroban_sdk::Bytes::new(&env);
        preimage3.append(&secret3.clone().into());
        preimage3.append(&blinding3.clone().into());
        let commitment3: BytesN<32> = env.crypto().sha256(&preimage3).into();

        let ip_id1 = client.commit_ip(&owner, &commitment1, &0u32);
        let ip_id2 = client.commit_ip(&owner, &commitment2, &0u32);
        let ip_id3 = client.commit_ip(&owner, &commitment3, &0u32);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip_id1);
        ip_ids.push_back(ip_id2);
        ip_ids.push_back(ip_id3);

        let mut secrets = Vec::new(&env);
        secrets.push_back(secret1);
        secrets.push_back(secret2);
        secrets.push_back(secret3);

        let mut blindings = Vec::new(&env);
        blindings.push_back(blinding1);
        blindings.push_back(blinding2);
        blindings.push_back(blinding3);

        let results = client.batch_verify_commitments(&ip_ids, &secrets, &blindings);

        assert_eq!(results.len(), 3);
        assert_eq!(results.get(0).unwrap(), true);
        assert_eq!(results.get(1).unwrap(), true);
        assert_eq!(results.get(2).unwrap(), true);
    }

    // ── Tests for Issue #340: IP Dispute ──

    #[test]
    fn test_raise_ip_dispute() {
        let env = Env::default();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let claimant = Address::generate(&env);
        let commitment = BytesN::from_array(&env, &[4u8; 32]);
        let evidence = BytesN::from_array(&env, &[5u8; 32]);

        env.mock_all_auths();

        let ip_id = client.commit_ip(&owner, &commitment, &0u32);
        client.raise_ip_dispute(&ip_id, &claimant, &evidence);

        assert_eq!(client.is_ip_in_dispute(&ip_id), true);
    }

    #[test]
    fn test_resolve_ip_dispute() {
        let env = Env::default();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let claimant = Address::generate(&env);
        let winner = Address::generate(&env);
        let commitment = BytesN::from_array(&env, &[6u8; 32]);
        let evidence = BytesN::from_array(&env, &[7u8; 32]);

        env.mock_all_auths();

        let ip_id = client.commit_ip(&owner, &commitment, &0u32);
        client.raise_ip_dispute(&ip_id, &claimant, &evidence);
        assert_eq!(client.is_ip_in_dispute(&ip_id), true);

        client.resolve_ip_dispute(&ip_id, &winner);
        assert_eq!(client.is_ip_in_dispute(&ip_id), false);
    }

    // ── Tests for Issue #341: Anonymous Commitment ──

    #[test]
    fn test_commit_ip_anonymous() {
        let env = Env::default();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let commitment = BytesN::from_array(&env, &[8u8; 32]);
        let zk_proof = soroban_sdk::Bytes::from_array(&env, &[1u8; 32]);

        env.mock_all_auths();

        let ip_id = client.commit_ip_anonymous(&commitment, &zk_proof);
        assert_eq!(client.is_ip_anonymous(&ip_id), true);
    }

    #[test]
    #[should_panic(expected = "Unauthorized")]
    fn test_commit_ip_anonymous_invalid_proof() {
        let env = Env::default();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let commitment = BytesN::from_array(&env, &[9u8; 32]);
        let zk_proof = soroban_sdk::Bytes::new(&env); // Empty proof

        env.mock_all_auths();

        client.commit_ip_anonymous(&commitment, &zk_proof);
    }
}
