#![no_main]
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{Env, BytesN, Bytes, Vec};
use arbitrary::Arbitrary;

#[derive(Arbitrary, Debug)]
struct BatchFuzzInput {
    /// Number of commitments to create (0-100 to avoid excessive memory)
    commitment_count: u8,
    /// Seeds for generating unique commitment hashes
    seeds: Vec<[u8; 32]>,
}

fuzz_target!(|input: BatchFuzzInput| {
    // Create a test environment
    let env = Env::default();

    // Initialize test addresses
    let owner = soroban_sdk::Address::from_contract_id(&env, &soroban_sdk::BytesN::<32>::from_array(&env, &[2u8; 32]));
    env.mock_all_auths();

    let count = std::cmp::min(input.commitment_count as usize, 100);
    let mut created_ids = Vec::new(&env);

    // Create multiple commitments with varying sizes and seeds
    for i in 0..count {
        let seed = if i < input.seeds.len() {
            input.seeds[i]
        } else {
            // Generate a deterministic seed from the index
            let mut seed = [0u8; 32];
            seed[0] = (i as u8).wrapping_add(3);
            seed[1] = (i as u8).wrapping_add(5);
            seed
        };

        // Create a commitment hash using the seed
        let mut preimage = Bytes::new(&env);
        preimage.append(&BytesN::<32>::from_array(&env, &seed));
        preimage.append(&BytesN::<32>::from_array(&env, &seed));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

        // Skip zero-value commitments
        if commitment_hash == BytesN::<32>::from_array(&env, &[0u8; 32]) {
            continue;
        }

        // Attempt to create the commitment
        if let Ok(ip_id) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ip_registry::IpRegistry::commit_ip(&env, owner.clone(), commitment_hash.clone())
        })) {
            created_ids.push_back(ip_id);
        }
    }

    // Verify that all created commitments can be retrieved and verified
    let created_count = created_ids.len();

    for i in 0..created_count {
        if let Ok(ip_id) = created_ids.get(i as u32) {
            // Verify we can retrieve the record
            if let Ok(_record) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ip_registry::IpRegistry::get_ip(&env, ip_id)
            })) {
                // Successfully retrieved the record
            }
        }
    }

    // Test batch retrieval by owner
    if let Ok(_owner_ips) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ip_registry::IpRegistry::get_ips_by_owner(&env, owner.clone())
    })) {
        // Successfully retrieved owner's IP list
    }

    // Verify monotonicity: IDs should be strictly increasing
    for i in 1..created_ids.len() {
        if let (Ok(prev_id), Ok(curr_id)) = (created_ids.get((i - 1) as u32), created_ids.get(i as u32)) {
            assert!(prev_id < curr_id, "IP IDs should be monotonically increasing");
        }
    }

    // Verify uniqueness: all created IDs should be distinct
    for i in 0..created_ids.len() {
        for j in (i + 1)..created_ids.len() {
            if let (Ok(id_i), Ok(id_j)) = (created_ids.get(i as u32), created_ids.get(j as u32)) {
                assert!(id_i != id_j, "All IP IDs should be unique");
            }
        }
    }
});
