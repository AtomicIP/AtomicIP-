# ADR-0002: Pedersen commitments for IP privacy

- **Status:** Accepted
- **Date:** 2026-09-26 (backfilled; decision predates this record)
- **Related issues / PRs:** #1063, #818
- **Supersedes:** —

## Context

An inventor must be able to prove they held an idea at a given time, and later
sell it, **without** publishing the idea. Everything written to a public
ledger is visible to everyone forever, including every transaction argument,
event and storage entry.

Requirements:

1. **Hiding:** the on-chain value must reveal nothing about the IP, even to
   an attacker who can guess likely contents.
2. **Binding:** after committing, the inventor cannot later claim a different
   secret.
3. **Selective disclosure:** ownership must be provable, and the secret
   revealable to a buyer during an atomic swap, without exposing it to everyone.
4. **Cheap verification** within Soroban resource limits
   (see `docs/soroban-resource-limits.md`).

## Options considered

### A. Plain hash `sha256(secret)`

- ✅ Cheapest; native host function
- ❌ Not hiding for low-entropy secrets: an attacker can brute-force candidate documents
- ❌ Proving ownership requires revealing the secret

### B. Blinded hash commitment `sha256(secret || blinding_factor)`

- ✅ Hiding (with a 32-byte random blinding factor) and binding (collision resistance)
- ✅ Native `sha256` host function; very cheap
- ❌ Verification still requires revealing `secret` and `blinding_factor` to the verifier

### C. Pedersen commitment `secret·G + blinding_factor·H` over Ristretto255

- ✅ Perfectly hiding, computationally binding (discrete log)
- ✅ Supports zero-knowledge proofs of knowledge of the opening (Schnorr/Okamoto),
  so ownership can be proven **without** revealing anything
- ✅ Additively homomorphic, which leaves room for future aggregate proofs
- ❌ More expensive to verify than a hash (curve operations in WASM)
- ❌ `H` must be generated with no known discrete log relative to `G`
  (nothing-up-my-sleeve derivation)

### D. General-purpose zk-SNARKs

- ✅ Most expressive
- ❌ Trusted setup / large verifiers; exceeds Soroban budget; heavy audit burden

## Decision

We use a **two-tier scheme**, together called the "Pedersen commitment scheme"
in the docs:

- **Default path (B):** `commitment_hash = sha256(secret || blinding_factor)`
  for `commit_ip`, swaps and `reveal_key`. This is cheap, and the atomic swap only
  reveals the opening to the buyer as part of settlement.
- **Zero-knowledge path (C):** real Pedersen commitments over Ristretto255 with
  a non-interactive Schnorr proof of knowledge (Fiat–Shamir) for
  `batch_verify_commitments`. Owners can prove they know the opening without
  ever putting `secret` or `blinding_factor` in a transaction.

Plain hashing (A) is rejected because it is not hiding. SNARKs (D) are rejected
because of cost and trusted-setup complexity.

## Consequences

### Positive

- IP content never appears on-chain in either path.
- Proving ownership without revealing anything is possible where it matters.
- The default path stays cheap enough for batch commits.

### Negative / trade-offs

- There are two commitment types to document and test. Differential tests
  (`contracts/ip_registry/src/differential_tests.rs`) guard their behaviour.
- Losing `secret` or `blinding_factor` makes a commitment unprovable. Key
  custody is the user's responsibility (see `docs/disaster-recovery.md`).
- The provers' nonces must never be reused, or the secret leaks. This is
  enforced off-chain only.

## Code references

- `contracts/ip_registry/src/zk_commitment.rs`: Pedersen + Schnorr verification
- `contracts/ip_registry/src/lib.rs`: `commit_ip`, `verify_commitment`, `batch_verify_commitments`
- `docs/commitment-scheme.md`: user-facing construction guide
