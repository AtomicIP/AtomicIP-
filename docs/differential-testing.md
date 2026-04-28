# Differential Testing Against Reference Implementation (#375)

## Overview

Differential testing runs the same inputs through two independent implementations
and asserts that their outputs agree. Any divergence reveals a logic bug in one
of them.

This project uses a **Python reference implementation** as the ground truth for
the core commitment scheme and state-machine logic.

## Reference Implementation

`tests/reference_impl.py` — pure Python, no external dependencies.

It implements:

| Component          | Python class / function          | Mirrors Rust                        |
|--------------------|----------------------------------|-------------------------------------|
| Commitment hash    | `commitment_hash(s, b)`          | `env.crypto().sha256(s \|\| b)`     |
| Verify commitment  | `verify_commitment(h, s, b)`     | `IpRegistry::verify_commitment`     |
| IP Registry        | `IpRegistry`                     | `contracts/ip_registry`             |
| Atomic Swap        | `AtomicSwap`                     | `contracts/atomic_swap`             |

## Running Differential Tests

### Python tests (reference implementation self-tests + differential assertions)

```bash
# With pytest
python3 -m pytest tests/test_differential.py -v

# Without pytest
python3 tests/test_differential.py
```

### Rust tests (verify Rust contract agrees with Python reference values)

```bash
cargo test differential_ -p ip_registry
```

## What Is Tested

| Test                                              | Catches                                      |
|---------------------------------------------------|----------------------------------------------|
| `commitment_hash_matches_sha256_concat`           | Wrong hash algorithm or argument order       |
| `commitment_hash_order_matters`                   | Commutative hash bug                         |
| `verify_commitment_correct_inputs`                | Always-false verify                          |
| `verify_commitment_wrong_secret`                  | Always-true verify                           |
| `commit_ip_returns_sequential_ids`                | Off-by-one in ID counter                     |
| `commit_ip_zero_hash_rejected`                    | Missing zero-hash guard                      |
| `commit_ip_duplicate_rejected`                    | Missing duplicate guard                      |
| `reveal_key_wrong_secret_raises`                  | Missing commitment verification in swap      |
| `reveal_key_on_pending_raises`                    | Wrong status check                           |
| `differential_hash_is_order_sensitive_like_python`| Argument order bug in Rust                   |

## Adding New Differential Tests

1. Add the expected behaviour to `tests/reference_impl.py`.
2. Add a Python test in `tests/test_differential.py` that exercises it.
3. Add a Rust test in `contracts/ip_registry/src/differential_tests.rs` (or
   `atomic_swap`) that asserts the Rust contract produces the same result.
4. Name Rust tests with the `differential_` prefix so they can be run in
   isolation.
