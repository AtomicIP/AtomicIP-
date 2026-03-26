#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN, Env, Vec};

// ── Storage Keys ────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    IpRecord(u64),
    OwnerIps(Address),
    NextId,
    CommitmentOwner(BytesN<32>), // tracks which owner already holds a commitment hash
}

// ── Types ────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct IpRecord {
    pub owner: Address,
    pub commitment_hash: BytesN<32>,
    pub timestamp: u64,
}

// ── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct IpRegistry;

#[contractimpl]
impl IpRegistry {
    /// Timestamp a new IP commitment. Returns the assigned IP ID.
    pub fn commit_ip(env: Env, owner: Address, commitment_hash: BytesN<32>) -> u64 {
        owner.require_auth();

        // Reject duplicate commitment hash globally
        assert!(
            !env.storage()
                .persistent()
                .has(&DataKey::CommitmentOwner(commitment_hash.clone())),
            "commitment already registered"
        );

        let id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(0);

        let record = IpRecord {
            owner: owner.clone(),
            commitment_hash: commitment_hash.clone(),
            timestamp: env.ledger().timestamp(),
        };

        env.storage().persistent().set(&DataKey::IpRecord(id), &record);
        env.storage()
            .persistent()
            .set(&DataKey::CommitmentOwner(commitment_hash), &owner);

        // Append to owner index
        let mut ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerIps(owner.clone()))
            .unwrap_or(Vec::new(&env));
        ids.push_back(id);
        env.storage().persistent().set(&DataKey::OwnerIps(owner), &ids);

        env.storage().instance().set(&DataKey::NextId, &(id + 1));

        env.events().publish(
            (symbol_short!("ip_commit"), owner),
            (id, record.timestamp),
        );

        id
    }

    /// Transfer IP ownership to a new address.
    pub fn transfer_ip(env: Env, ip_id: u64, new_owner: Address) {
        let mut record: IpRecord = env
            .storage()
            .persistent()
            .get(&DataKey::IpRecord(ip_id))
            .expect("IP not found");

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
            .set(&DataKey::OwnerIps(old_owner), &old_ids);

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

        // Update commitment index
        env.storage()
            .persistent()
            .set(&DataKey::CommitmentOwner(record.commitment_hash.clone()), &new_owner);

        record.owner = new_owner;
        env.storage().persistent().set(&DataKey::IpRecord(ip_id), &record);
    }

    /// Retrieve an IP record by ID.
    pub fn get_ip(env: Env, ip_id: u64) -> IpRecord {
        env.storage()
            .persistent()
            .get(&DataKey::IpRecord(ip_id))
            .expect("IP not found")
    }

    /// Verify a commitment: recompute sha256(secret || blinding_factor) and compare to stored hash.
    pub fn verify_commitment(
        env: Env,
        ip_id: u64,
        secret: BytesN<32>,
        blinding_factor: BytesN<32>,
    ) -> bool {
        let record: IpRecord = env
            .storage()
            .persistent()
            .get(&DataKey::IpRecord(ip_id))
            .expect("IP not found");

        let mut preimage = Bytes::new(&env);
        preimage.append(&secret.into());
        preimage.append(&blinding_factor.into());

        let computed: BytesN<32> = env.crypto().sha256(&preimage).into();
        record.commitment_hash == computed
    }

    /// List all IP IDs owned by an address.
    /// Returns `None` if the address has never committed any IP.
    pub fn list_ip_by_owner(env: Env, owner: Address) -> Option<Vec<u64>> {
        env.storage().persistent().get(&DataKey::OwnerIps(owner))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn commit_ip_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[0xabu8; 32]);
        let id = client.commit_ip(&owner, &hash);

        let events = env.events().all();
        assert_eq!(events.len(), 1);
        let (_, topics, data): (_, soroban_sdk::Vec<soroban_sdk::Val>, soroban_sdk::Val) =
            events.get(0).unwrap();
        // topic[1] is the owner address; data is (ip_id, timestamp)
        let (emitted_id, _timestamp): (u64, u64) =
            soroban_sdk::FromVal::from_val(&env, &data);
        assert_eq!(emitted_id, id);
    }

    #[test]
    fn unknown_owner_returns_none() {
        let env = Env::default();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let stranger = Address::generate(&env);
        assert_eq!(client.list_ip_by_owner(&stranger), None);
    }

    #[test]
    fn commitment_verifies_with_correct_secret_and_blinding() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let secret = BytesN::from_array(&env, &[0xabu8; 32]);
        let blinding = BytesN::from_array(&env, &[0xcdu8; 32]);

        // Build commitment off-chain: sha256(secret || blinding)
        let mut preimage = Bytes::new(&env);
        preimage.append(&Bytes::from(secret.clone()));
        preimage.append(&Bytes::from(blinding.clone()));
        let commitment: BytesN<32> = env.crypto().sha256(&preimage).into();

        let id = client.commit_ip(&owner, &commitment);

        assert!(client.verify_commitment(&id, &secret, &blinding));
        // Wrong blinding factor must fail
        let wrong = BytesN::from_array(&env, &[0x00u8; 32]);
        assert!(!client.verify_commitment(&id, &secret, &wrong));
    }

    #[test]
    fn known_owner_returns_some() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let id = client.commit_ip(&owner, &hash);

        let ids = client.list_ip_by_owner(&owner).expect("should be Some");
        assert_eq!(ids.len(), 1);
        assert_eq!(ids.get(0).unwrap(), id);
    }
}
