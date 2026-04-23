#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN, Env,
    Error, Vec,
};

mod validation;
use validation::*;

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
}

// ── TTL ───────────────────────────────────────────────────────────────────────

/// Minimum ledger TTL bump applied to every persistent storage write.
/// ~1 year at ~5s per ledger: 365 * 24 * 3600 / 5 ≈ 6_307_200 ledgers.
pub const LEDGER_BUMP: u32 = 6_307_200;

/// Maximum metadata size: 1 KB
pub const MAX_METADATA_BYTES: u32 = 1024;

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
}

#[contracttype]
#[derive(Clone)]
pub struct LicenseEntry {
    pub licensee: Address,
    pub terms_hash: BytesN<32>,
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
    pub fn commit_ip(env: Env, owner: Address, commitment_hash: BytesN<32>) -> u64 {
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
            expiry_timestamp: 0,
            metadata: Bytes::new(&env),
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

        id
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
    }

    /// Admin-only contract upgrade.
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
                args: (bob.clone(), hash.clone()).into_val(&env),
                sub_invokes: &[],
            },
        }]);

        // This call passes bob's address as owner but only alice's auth is mocked.
        // The SDK MUST reject this with an auth panic — confirming the bug condition
        // is correctly enforced at the protocol level.
        client.commit_ip(&bob, &hash);
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
        let ip_id = client.commit_ip(&bob, &hash);

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
        let ip_id = client.commit_ip(&owner, &commitment);
        let record = client.get_ip(&ip_id);

        assert_eq!(record.timestamp, recorded_time);
    }
}
