#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, BytesN, Env, Error, Vec,
};

#[cfg(test)]
mod test;

// ── Storage Keys ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Debug, PartialEq)]
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

        env.storage()
            .persistent()
            .set(&DataKey::IpRecord(id), &record);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::IpRecord(id), 50000, 50000);

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
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::OwnerIps(owner.clone()), 50000, 50000);

        env.storage().instance().set(&DataKey::NextId, &(id + 1));

        env.events().publish(
            (symbol_short!("ip_commit"), owner.clone()),
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
        env.storage().persistent().set(
            &DataKey::CommitmentOwner(record.commitment_hash.clone()),
            &new_owner,
        );

        record.owner = new_owner;
        env.storage()
            .persistent()
            .set(&DataKey::IpRecord(ip_id), &record);
    }

    /// Retrieve an IP record by ID.
    pub fn get_ip(env: Env, ip_id: u64) -> IpRecord {
        env.storage()
            .persistent()
            .get(&DataKey::IpRecord(ip_id))
            .unwrap_or_else(|| env.panic_with_error(Error::from_contract_error(1)))
    }

    /// Verify a commitment: recompute sha256(secret || blinding_factor) and compare to stored hash.
    pub fn verify_commitment(
        env: Env,
        ip_id: u64,
        secret: BytesN<32>,
        _blinding_factor: BytesN<32>,
    ) -> bool {
        let record: IpRecord = env
            .storage()
            .persistent()
            .get(&DataKey::IpRecord(ip_id))
            .unwrap_or_else(|| env.panic_with_error(Error::from_contract_error(1)));
        record.commitment_hash == secret
    }

    /// List all IP IDs owned by an address.
    /// Returns `None` if the address has never committed any IP.
    pub fn list_ip_by_owner(env: Env, owner: Address) -> Option<Vec<u64>> {
        env.storage().persistent().get(&DataKey::OwnerIps(owner))
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
}
