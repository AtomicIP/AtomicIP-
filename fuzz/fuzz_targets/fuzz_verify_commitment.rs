#![no_main]
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{Env, BytesN, Bytes};
use ip_registry::IpRegistry;
use arbitrary::Arbitrary;

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    secret: [u8; 32],
    blinding_factor: [u8; 32],
}

fuzz_target!(|input: FuzzInput| {
    // Create a test environment
    let env = Env::default();

    // Initialize admin and owner
    let admin = soroban_sdk::Address::from_contract_id(&env, &env.contract_id());
    let owner = soroban_sdk::Address::from_contract_id(&env, &soroban_sdk::BytesN::<32>::from_array(&env, &[1u8; 32]));

    // Mock authentication for testing
    env.mock_all_auths();

    // Create a commitment hash from the fuzzed input
    let mut preimage = Bytes::new(&env);
    preimage.append(&BytesN::<32>::from_array(&env, &input.secret));
    preimage.append(&BytesN::<32>::from_array(&env, &input.blinding_factor));
    let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

    // Skip if commitment hash is all zeros (invalid)
    if commitment_hash == BytesN::<32>::from_array(&env, &[0u8; 32]) {
        return;
    }

    // Attempt to commit the IP (may fail if constraints are violated)
    let contract = IpRegistry;
    if let Ok(ip_id) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ip_registry::IpRegistry::commit_ip(&env, owner.clone(), commitment_hash.clone())
    })) {
        // If commitment succeeded, verify it works
        let secret_bytes = BytesN::<32>::from_array(&env, &input.secret);
        let blinding_bytes = BytesN::<32>::from_array(&env, &input.blinding_factor);

        if let Ok(is_valid) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ip_registry::IpRegistry::verify_commitment(&env, ip_id, secret_bytes.clone(), blinding_bytes.clone())
        })) {
            // Verification should always succeed with the correct inputs
            assert!(is_valid, "Verification should succeed with correct inputs");
        }

        // Also verify that wrong inputs fail
        let wrong_secret = BytesN::<32>::from_array(&env, &{
            let mut arr = input.secret;
            arr[0] = arr[0].wrapping_add(1);
            arr
        });

        if let Ok(is_invalid) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ip_registry::IpRegistry::verify_commitment(&env, ip_id, wrong_secret, blinding_bytes.clone())
        })) {
            // Wrong secret should not verify
            assert!(!is_invalid, "Verification should fail with wrong secret");
        }
    }
});
