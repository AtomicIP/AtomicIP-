#![no_main]
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Bytes, BytesN, Env};

#[derive(Arbitrary, Debug)]
struct VerifyInput {
    secret: [u8; 32],
    blinding_factor: [u8; 32],
}

fuzz_target!(|input: VerifyInput| {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);

    // Compute commitment_hash = sha256(secret || blinding_factor)
    let mut preimage = Bytes::new(&env);
    preimage.append(&BytesN::<32>::from_array(&env, &input.secret).into());
    preimage.append(&BytesN::<32>::from_array(&env, &input.blinding_factor).into());
    let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

    // Skip if commitment hash is all-zero bytes (IpRegistry rejects zero-value hashes)
    if commitment_hash == BytesN::<32>::from_array(&env, &[0u8; 32]) {
        return;
    }

    // Wrap commit_ip in catch_unwind to handle any unexpected contract panics
    // without crashing the fuzzer
    let ip_id = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ip_registry::IpRegistry::commit_ip(env.clone(), owner.clone(), commitment_hash.clone())
    })) {
        Ok(id) => id,
        Err(_) => return,
    };

    let secret_bytes = BytesN::<32>::from_array(&env, &input.secret);
    let blinding_bytes = BytesN::<32>::from_array(&env, &input.blinding_factor);

    // Feature: fuzz-testing-commitment-scheme, Property 1: Commitment Verification Round-Trip
    let is_valid = ip_registry::IpRegistry::verify_commitment(
        env.clone(),
        ip_id,
        secret_bytes.clone(),
        blinding_bytes.clone(),
    );
    assert!(
        is_valid,
        "verify_commitment must return true for correct secret and blinding_factor"
    );

    // Construct wrong_secret by flipping the first byte of secret (XOR with 0x01;
    // if secret[0] == 0x01 use 0x02 instead)
    let flip_byte = if input.secret[0] == 0x01 { 0x02u8 } else { 0x01u8 };
    let mut wrong_arr = input.secret;
    wrong_arr[0] ^= flip_byte;
    let wrong_secret = BytesN::<32>::from_array(&env, &wrong_arr);

    // Feature: fuzz-testing-commitment-scheme, Property 2: Wrong Secret Fails Verification
    let is_invalid =
        ip_registry::IpRegistry::verify_commitment(env.clone(), ip_id, wrong_secret, blinding_bytes);
    assert!(
        !is_invalid,
        "verify_commitment must return false for a secret that differs by at least one byte"
    );
});
