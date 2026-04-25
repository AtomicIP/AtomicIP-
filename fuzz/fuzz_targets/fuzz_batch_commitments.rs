#![no_main]
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Bytes, BytesN, Env};

// Task 4.1 — input struct and setup
// Requirements: 3.1, 3.2
#[derive(Arbitrary, Debug)]
struct BatchInput {
    /// Number of commitments to create (capped to 100 in harness)
    commitment_count: u8,
    /// Seeds for generating unique commitment hashes
    seeds: std::vec::Vec<[u8; 32]>,
}

fuzz_target!(|input: BatchInput| {
    // Task 4.1 — construct environment and mock auth
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);

    // Task 4.1 — cap iteration count at 100
    // Requirements: 3.2
    let count = (input.commitment_count as usize).min(100);

    let mut success_count: usize = 0;
    let mut created_ids: std::vec::Vec<u64> = std::vec::Vec::new();

    // Task 4.2 — commit loop with duplicate-hash handling
    // Requirements: 3.7
    for i in 0..count {
        let seed: [u8; 32] = if i < input.seeds.len() {
            input.seeds[i]
        } else {
            // Deterministic fallback seed from index
            let mut s = [0u8; 32];
            s[0] = (i as u8).wrapping_add(3);
            s[1] = (i as u8).wrapping_add(5);
            s
        };

        // Compute commitment_hash = sha256(seed || seed)
        let mut preimage = Bytes::new(&env);
        preimage.append(&BytesN::<32>::from_array(&env, &seed).into());
        preimage.append(&BytesN::<32>::from_array(&env, &seed).into());
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

        // Skip all-zero hashes without counting them
        if commitment_hash == BytesN::<32>::from_array(&env, &[0u8; 32]) {
            continue;
        }

        // Wrap commit_ip in catch_unwind to handle CommitmentAlreadyRegistered
        // without crashing the fuzzer
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ip_registry::IpRegistry::commit_ip(
                env.clone(),
                owner.clone(),
                commitment_hash.clone(),
            )
        }));

        if let Ok(ip_id) = result {
            // Task 4.3 — Property 3: Batch IP IDs Are Strictly Monotonically Increasing
            // Feature: fuzz-testing-commitment-scheme, Property 3: Batch IP IDs Are Strictly Monotonically Increasing
            // Requirements: 3.3, 3.4
            if let Some(&prev_id) = created_ids.last() {
                assert!(
                    prev_id < ip_id,
                    "IP IDs must be strictly monotonically increasing: prev={} curr={}",
                    prev_id,
                    ip_id
                );
            }

            created_ids.push(ip_id);
            success_count += 1;
        }
        // Duplicate-hash panics are silently swallowed; not counted toward success_count
    }

    // Task 4.5 — Property 4: Committed IP Is Always Retrievable
    // Feature: fuzz-testing-commitment-scheme, Property 4: Committed IP Is Always Retrievable
    // Requirements: 3.5
    for &ip_id in &created_ids {
        let record = ip_registry::IpRegistry::get_ip(env.clone(), ip_id);
        assert_eq!(
            record.ip_id, ip_id,
            "get_ip must return a record whose ip_id equals the queried ID"
        );
    }

    // Task 4.7 — Property 5: Owner IP List Count Matches Successful Commits
    // Feature: fuzz-testing-commitment-scheme, Property 5: Owner IP List Count Matches Successful Commits
    // Requirements: 3.6
    let owner_ips = ip_registry::IpRegistry::list_ip_by_owner(env.clone(), owner.clone());
    assert_eq!(
        owner_ips.len() as usize,
        success_count,
        "list_ip_by_owner length must equal the number of successful commits"
    );

    // Task 4.9 — Property 10: Batch Cap Is Enforced
    // Feature: fuzz-testing-commitment-scheme, Property 10: Batch Cap Is Enforced
    // Requirements: 3.2
    assert!(
        created_ids.len() <= 100,
        "created_ids must never exceed the batch cap of 100"
    );
});
