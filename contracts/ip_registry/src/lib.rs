#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, Vec};

// ── Storage Keys ────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    IpRecord(u64),
    OwnerIps(Address),
    NextId,
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

        const TTL_THRESHOLD: u32 = 518400;
        const TTL_BUMP: u32 = 518400;

        let id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NextId)
            .unwrap_or(0);

        let record = IpRecord {
            owner: owner.clone(),
            commitment_hash,
            timestamp: env.ledger().timestamp(),
        };

        env.storage().persistent().set(&DataKey::IpRecord(id), &record);

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
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn test_next_id_counter_persists_across_calls() {
        let env = Env::default();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[0u8; 32]);

        env.mock_all_auths();
        let id0 = client.commit_ip(&owner, &hash);
        let id1 = client.commit_ip(&owner, &hash);
        let id2 = client.commit_ip(&owner, &hash);

        assert_eq!(id0, 0);
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);

        // counter is in persistent storage — verify directly
        env.as_contract(&contract_id, || {
            let next: u64 = env
                .storage()
                .persistent()
                .get(&DataKey::NextId)
                .expect("NextId missing from persistent storage");
            assert_eq!(next, 3);
        });
    }
}
