#![cfg(test)]

use soroban_sdk::{Address, BytesN, Env};
use soroban_sdk::testutils::Address as TestAddress;
use crate::IpRegistry;

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_get_ip_panics_on_nonexistent_id() {
    let env = Env::default();
    let contract_id = env.register_contract(None, IpRegistry);
    
    // Try to get an IP with ID 999 that has never been committed
    // This should panic with contract error 1
    env.as_contract(&contract_id, || {
        IpRegistry::get_ip(env.clone(), 999u64);
    });
}

#[test]
fn test_get_ip_success_after_commit() {
    let env = Env::default();
    let contract_id = env.register_contract(None, IpRegistry);
    
    let owner = Address::generate(&env);
    let commitment_hash = BytesN::from_array(&env, &[1u8; 32]);
    
    // Commit an IP first (mock the authorization)
    env.mock_all_auths();
    
    let ip_id = env.as_contract(&contract_id, || {
        IpRegistry::commit_ip(env.clone(), owner.clone(), commitment_hash.clone())
    });
    
    // Now get_ip should work
    let record = env.as_contract(&contract_id, || {
        IpRegistry::get_ip(env.clone(), ip_id)
    });
    assert_eq!(record.owner, owner);
    assert_eq!(record.commitment_hash, commitment_hash);
}
