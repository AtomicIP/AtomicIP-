# ADR-0003: Stellar / Soroban for settlement

- **Status:** Accepted
- **Date:** 2026-09-26 (backfilled; decision predates this record)
- **Related issues / PRs:** #1063

## Context

AtomicIP needs a public ledger to:

1. timestamp IP commitments immutably (proof of prior art);
2. settle patent/IP sales **atomically**: the buyer's payment and the
   seller's key reveal either both happen or neither does;
3. support stable-value payment (USDC) as well as the native asset;
4. keep per-transaction cost low enough for individual inventors and for batch
   commits.

## Options considered

### A. Ethereum L1 (Solidity)

- ✅ Largest ecosystem and tooling
- ❌ Fees too high and too variable for small inventors; batch commits are costly

### B. Ethereum L2 (rollup)

- ✅ Lower fees, EVM tooling
- ❌ Finality and withdrawal delays; sequencer trust assumptions; bridged USDC variants

### C. Stellar with Soroban smart contracts (Rust → WASM)

- ✅ ~5s finality and predictable, very low fees
- ✅ Native issued assets, including Circle USDC, and a built-in token interface (SAC)
- ✅ Rust contracts: memory safety, and shared crypto crates with the API server
- ✅ Explicit resource metering and state archival/TTL model
- ❌ Smaller ecosystem; fewer auditors and indexers
- ❌ Storage entries expire unless TTL is extended (operational burden)
- ❌ Tighter CPU/memory budgets constrain on-chain cryptography

### D. Private / permissioned ledger

- ❌ Loses public, trust-minimised timestamping, which is the core value

## Decision

We settle on **Stellar**, using **Soroban** smart contracts:

- `ip_registry` stores commitments, ownership and the audit trail.
- `atomic_swap` escrows payment tokens and releases them when the seller reveals
  a valid key, with cancellation/expiry paths for both parties.

## Consequences

### Positive

- Atomic, low-cost settlement with native USDC support.
- Fast finality makes swap UX responsive.
- Contracts and API server share the Rust toolchain.

### Negative / trade-offs

- Contract storage TTLs must be monitored and extended, or records get
  archived (see `docs/disaster-recovery.md`, "Ledger state archival").
- On-chain cryptography must fit Soroban budgets
  (`docs/soroban-resource-limits.md`). This shaped ADR-0002.
- Depends on Soroban RPC availability. The API server includes circuit breaker
  and fallback logic (`api-server/src/circuit_breaker.rs`, `fallback.rs`).

## Code references

- `contracts/ip_registry/src/lib.rs`
- `contracts/atomic_swap/src/lib.rs`
- `api-server/src/soroban_rpc.rs`
