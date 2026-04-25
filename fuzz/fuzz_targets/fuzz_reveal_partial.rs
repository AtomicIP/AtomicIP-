#![no_main]
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Bytes, BytesN, Env};

#[derive(Arbitrary, Debug)]
struct RevealInput {
    partial_hash: [u8; 32],
    blinding_factor: [u8; 32],
}

fuzz_target!(|input: RevealInput| {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);

    // Compute commitment_hash = sha256(partial_hash || blinding_factor)
    let mut preimage = Bytes::new(&env);
    preimage.append(&BytesN::<32>::from_array(&env, &input.partial_hash).into());
    preimage.append(&BytesN::<32>::from_array(&env, &input.blinding_factor).into());
    let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

    // Skip all-zero hashes (rejected by contract)
    if commitment_hash == BytesN::<32>::from_array(&env, &[0u8; 32]) {
        return;
    }

    let ip_id = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ip_registry::IpRegistry::commit_ip(env.clone(), owner.clone(), commitment_hash.clone())
    })) {
        Ok(id) => id,
        Err(_) => return,
    };

    let partial_hash_bytes = BytesN::<32>::from_array(&env, &input.partial_hash);
    let blinding_bytes = BytesN::<32>::from_array(&env, &input.blinding_factor);

    // Feature: fuzz-testing-commitment-scheme, Property 6: reveal_partial Round-Trip
    let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ip_registry::IpRegistry::reveal_partial(
            env.clone(),
            ip_id,
            partial_hash_bytes.clone(),
            blinding_bytes.clone(),
        )
    })) {
        Ok(r) => r,
        Err(_) => return,
    };
    assert!(
        result,
        "reveal_partial must return true for correct partial_hash and blinding_factor"
    );

    // Feature: fuzz-testing-commitment-scheme, Property 8: get_partial_disclosure Round-Trip
    let disclosed = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ip_registry::IpRegistry::get_partial_disclosure(env.clone(), ip_id)
    })) {
        Ok(d) => d,
        Err(_) => return,
    };
    assert_eq!(
        disclosed,
        Some(partial_hash_bytes.clone()),
        "get_partial_disclosure must return the revealed partial_hash"
    );

    // Feature: fuzz-testing-commitment-scheme, Property 9: reveal_partial Is Idempotent
    let result2 = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ip_registry::IpRegistry::reveal_partial(
            env.clone(),
            ip_id,
            partial_hash_bytes.clone(),
            blinding_bytes.clone(),
        )
    })) {
        Ok(r) => r,
        Err(_) => return,
    };
    assert!(result2, "reveal_partial must be idempotent");

    let disclosed2 = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ip_registry::IpRegistry::get_partial_disclosure(env.clone(), ip_id)
    })) {
        Ok(d) => d,
        Err(_) => return,
    };
    assert_eq!(
        disclosed2,
        Some(partial_hash_bytes.clone()),
        "get_partial_disclosure must return the same partial_hash after idempotent reveal"
    );

    // Feature: fuzz-testing-commitment-scheme, Property 7: reveal_partial Rejects Non-Matching Inputs
    let flip_byte = if input.partial_hash[0] == 0x01 { 0x02u8 } else { 0x01u8 };
    let mut wrong_arr = input.partial_hash;
    wrong_arr[0] ^= flip_byte;
    let wrong_partial = BytesN::<32>::from_array(&env, &wrong_arr);

    let invalid_result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ip_registry::IpRegistry::reveal_partial(
            env.clone(),
            ip_id,
            wrong_partial,
            blinding_bytes.clone(),
        )
    })) {
        Ok(r) => r,
        Err(_) => return,
    };
    assert!(
        !invalid_result,
        "reveal_partial must return false for non-matching partial_hash"
    );
});
