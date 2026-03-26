#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, Vec};

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

        env.storage().persistent().set(&DataKey::NextId, &(id + 1));
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::NextId, TTL_THRESHOLD, TTL_BUMP);
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

    /// Verify a commitment: hash the secret and compare to stored commitment.
    pub fn verify_commitment(env: Env, ip_id: u64, secret: BytesN<32>) -> bool {
        let record: IpRecord = env
            .storage()
            .persistent()
            .get(&DataKey::IpRecord(ip_id))
            .expect("IP not found");
        record.commitment_hash == secret
    }

    /// List all IP IDs owned by an address.
    pub fn list_ip_by_owner(env: Env, owner: Address) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::OwnerIps(owner))
            .unwrap_or(Vec::new(&env))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

    #[test]
    fn test_transfer_ip_updates_owner_and_index() {
        let env = Env::default();
        env.mock_all_auths();

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);

        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let ip_id = client.commit_ip(&alice, &hash);
        client.transfer_ip(&ip_id, &bob);

        let record = client.get_ip(&ip_id);
        assert_eq!(record.owner, bob);

        // alice's index should be empty, bob's should contain the id
        assert_eq!(client.list_ip_by_owner(&alice).len(), 0);
        assert_eq!(client.list_ip_by_owner(&bob).get(0), Some(ip_id));
    }

    #[test]
    #[should_panic(expected = "commitment already registered")]
    fn test_duplicate_commitment_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[2u8; 32]);

        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        client.commit_ip(&alice, &hash);
        client.commit_ip(&bob, &hash); // same hash — must panic
    }
}
