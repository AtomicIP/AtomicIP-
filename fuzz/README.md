# Fuzz Testing for IP Registry

This directory contains fuzz tests for the Pedersen commitment scheme implementation in the IP Registry contract.

## Overview

Fuzzing helps identify edge cases and potential vulnerabilities in cryptographic operations by testing with random, automatically generated inputs.

## Prerequisites

- Rust 1.70+
- cargo-fuzz: `cargo install cargo-fuzz`
- libfuzzer support (usually available on modern Rust installations)

## Building Fuzz Tests

```bash
cd fuzz
cargo fuzz build
```

## Running Fuzz Tests

### Fuzz Target 1: verify_commitment

Tests the Pedersen commitment verification function with random secrets and blinding factors.

```bash
cd fuzz
cargo fuzz run fuzz_verify_commitment -- -max_len=64 -timeout=1
```

Run for extended period (e.g., 1 hour):

```bash
timeout 3600 cargo fuzz run fuzz_verify_commitment -- -timeout=2 -max_len=64
```

**What it tests:**
- Random secret values (32 bytes)
- Random blinding factors (32 bytes)
- Correct verification with valid inputs
- Rejection of modified secrets or blinding factors
- Edge cases: all zeros, all ones, repeated patterns
- Hash collision resistance

### Fuzz Target 2: batch_commitments

Tests batch commitment creation and retrieval with varying batch sizes.

```bash
cd fuzz
cargo fuzz run fuzz_batch_commitments -- -max_len=256 -timeout=1
```

Run for extended period:

```bash
timeout 3600 cargo fuzz run fuzz_batch_commitments -- -timeout=2 -max_len=256
```

**What it tests:**
- Creating 0-100 commitments per fuzz iteration
- Monotonic ID assignment
- Uniqueness of commitment hashes
- Batch retrieval by owner
- Edge cases: empty batch, single commitment, maximum batch size
- Storage consistency across multiple operations

## Interpreting Results

### Successful Run

If fuzzing completes without finding issues:
- No panic or assertion failures
- All test conditions pass consistently
- May show coverage information

### Finding Issues

If a bug is found, cargo-fuzz will:
1. Display the failing input
2. Save the failing case to `fuzz/artifacts/fuzz_target_name/crash-*`
3. Allow reproduction with: `cargo fuzz run fuzz_target_name fuzz/artifacts/...`

## Fuzzing Strategy

The fuzz tests focus on:

1. **Cryptographic Integrity**
   - Verify that commitment hashes are computed correctly
   - Ensure that modification of inputs changes the hash
   - Test collision handling

2. **State Management**
   - Verify monotonic ID assignment
   - Ensure commitment uniqueness
   - Check storage consistency

3. **Soroban-Specific Concerns**
   - TTL management and ledger bumping
   - Large batch handling
   - Memory constraints

## Integration with CI/CD

The CI system should run fuzz tests periodically:

```bash
# In CI pipeline
timeout 1h cargo fuzz run fuzz_verify_commitment -- -timeout=2 &
timeout 1h cargo fuzz run fuzz_batch_commitments -- -timeout=2 &
wait
```

## Notes

- Fuzz tests use `Arbitrary` crate for generating structured inputs
- Tests use `panic::catch_unwind` to handle Soroban panics gracefully
- All tests run with `env.mock_all_auths()` for test simplicity
- Random seed can be controlled via environment: `LIBFUZZER_SEED=12345`

## See Also

- [libfuzzer Docmentation](https://llvm.org/docs/LibFuzzer/)
- [cargo-fuzz Book](https://rust-fuzz.github.io/book/cargo-fuzz.html)
- [Commitment Scheme Documentation](../docs/commitment-scheme.md)
