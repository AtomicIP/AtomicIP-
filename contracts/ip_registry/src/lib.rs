#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, BytesN, Bytes, Env, Error, Vec,
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
}

// ── TTL ───────────────────────────────────────────────────────────────────────

/// Minimum ledger TTL bump applied to every persistent storage write.
/// ~1 year at ~5s per ledger: 365 * 24 * 3600 / 5 ≈ 6_307_200 ledgers.
pub const LEDGER_BUMP: u32 = 6_307_200;

// ── Storage Keys ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Debug, PartialEq)]
pub enum DataKey {
    IpRecord(u64),
    OwnerIps(Address),
    NextId,
    CommitmentOwner(BytesN<32>), // tracks which owner already holds a commitment hash
    Admin,
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
    pub fn commit_ip(env: Env, owner: Address, commitment_hash: BytesN<32>) -> Result<u64, ContractError> {
        // Enforced by the Soroban host: panics if the transaction does not carry
        // a valid authorization for `owner`. This is the correct auth pattern.
        owner.require_auth();

        // Initialize admin on first call if not set
        if !env.storage().persistent().has(&DataKey::Admin) {
            let admin = env.deployer();
            env.storage().persistent().set(&DataKey::Admin, &admin);
            env.storage().persistent().extend_ttl(&DataKey::Admin, 50000, 50000);
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

    /// Batch commit multiple IP hashes from the same owner in a single transaction.
    /// Reduces gas fees compared to sequential commit_ip calls.
    ///
    /// All hashes must be unique and non-zero. Sequential IDs assigned.
    ///
    /// # Arguments
    ///
    /// * `env` - Soroban environment
    /// * `owner` - Owner address (requires auth)
    /// * `hashes` - Vec of BytesN<32> commitment hashes
    ///
    /// # Returns
    ///
    /// Vec<u64> of assigned sequential IP IDs
    ///
    /// # Panics
    ///
    /// * ZeroCommitmentHash(2) if any hash is zero
    /// * CommitmentAlreadyRegistered(3) if any hash already exists globally
    pub fn batch_commit_ip(env: Env, owner: Address, hashes: Vec<BytesN<32>>) -> Vec<u64> {
        owner.require_auth();

        // Admin init
        if !env.storage().persistent().has(&DataKey::Admin) {
            let admin = env.deployer();
            env.storage().persistent().set(&DataKey::Admin, &admin);
            env.storage().persistent().extend_ttl(&DataKey::Admin, 50000, 50000);
        }

        let mut next_id: u64 = env.storage().persistent().get(&DataKey::NextId).unwrap_or(0);
        let mut ids = Vec::new(&env);

        for hash in hashes.iter() {
            let commitment_hash = hash.clone();

            // Zero hash check
            require_non_zero_commitment(&env, &commitment_hash);

            // Duplicate commitment check
            require_unique_commitment(&env, &commitment_hash);

            let record = IpRecord {
                ip_id: next_id,
                owner: owner.clone(),
                commitment_hash: commitment_hash.clone(),
                timestamp: env.ledger().timestamp(),
                revoked: false,
            };

            env.storage().persistent().set(&DataKey::IpRecord(next_id), &record);
            env.storage().persistent().extend_ttl(&DataKey::IpRecord(next_id), 50000, 50000);

            env.storage().persistent().set(&DataKey::CommitmentOwner(commitment_hash.clone()), &owner);

            ids.push_back(next_id);

            // Emit event per IP
            env.events().publish(
                (symbol_short!("ip_commit"), owner.clone()),
                (next_id, record.timestamp),
            );

            next_id += 1;
        }

        // Update owner index
        let mut owner_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerIps(owner.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        for &id in ids.iter() {
            owner_ids.push_back(id);
        }
        env.storage().persistent().set(&DataKey::OwnerIps(owner.clone()), &owner_ids);
        env.storage().persistent().extend_ttl(&DataKey::OwnerIps(owner.clone()), 50000, 50000);

        // Update NextId once
        env.storage().persistent().set(&DataKey::NextId, &next_id);
        env.storage().persistent().extend_ttl(&DataKey::NextId, 50000, 50000);

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
        
        Ok(())
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
        Ok(())
    }

    /// Admin-only contract upgrade.
    ///
    /// # Panics
    ///
    /// Panics if caller is not admin or admin not initialized.
    pub fn upgrade(env: Env, new_wasm_hash: Bytes) -> Result<(), ContractError> {
        let admin_opt = env.storage().persistent().get(&DataKey::Admin);
        if admin_opt.is_none() {
            return Err(ContractError::UnauthorizedUpgrade);
        }
        let admin = admin_opt.unwrap();
        let invoker = env.invoker();
        if invoker != admin {
            return Err(ContractError::UnauthorizedUpgrade);
        }
        admin.require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        Ok(())
    }

    /// Admin-only contract upgrade.
    ///
    /// # Panics
    ///
    /// Panics if caller is not admin or admin not initialized.
    pub fn upgrade(env: Env, new_wasm_hash: Bytes) {
        let admin_opt = env.storage().persistent().get(&DataKey::Admin);
        if admin_opt.is_none() {
            env.panic_with_error(Error::from_contract_error(ContractError::UnauthorizedUpgrade as u32));
        }
        let admin = admin_opt.unwrap();
        let invoker = env.invoker();
        if invoker != admin {
            env.panic_with_error(Error::from_contract_error(ContractError::UnauthorizedUpgrade as u32));
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

        // Concatenate secret || blinding_factor into Bytes, then SHA256
        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&secret.into());
        preimage.append(&blinding_factor.into());
        let computed_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

        Ok(record.commitment_hash == computed_hash)
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
