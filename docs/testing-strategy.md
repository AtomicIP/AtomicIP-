# Test Suite Strategy

This document provides an overview of the current test suite status across the AtomicIP project, including enabled and disabled test modules, their purposes, and cleanup tracking.

## Overview

The test suite is distributed across the Soroban contracts (`ip_registry`, `atomic_swap`) and JavaScript integration layer (`api-server`). Some test modules are currently disabled pending refactoring or protocol updates.

## IP Registry Contract Tests

**Location:** `contracts/ip_registry/src/`

### Enabled Test Modules

| Module | Status | Purpose | Lines |
|---|---|---|---|
| `test` | ✅ Enabled | Core functionality tests for IP commitment, transfer, and revocation | ~500 |
| `benchmarks` | ✅ Enabled | Performance benchmarks for critical operations (issue #817) | ~200 |
| `mutation_tests` | ✅ Enabled | Mutation testing to validate test quality | ~150 |
| `snapshot_tests` | ✅ Enabled | State snapshot comparisons for storage migrations | ~100 |
| `differential_tests` | ✅ Enabled | Comparison tests between legacy and new storage layouts | ~150 |
| `invariant_tests` | ✅ Enabled | Protocol invariant validation (sharding, shard limits) | ~180 |
| `upgrade_tests` | ✅ Enabled | Contract upgrade and data migration scenarios | ~120 |

### Notes

- All test modules in `ip_registry` are actively maintained and run in CI.
- `differential_tests` validate the migration path between legacy `ShardIps` and bounded `ShardSubIps` layouts (see architecture.md for details).

## Atomic Swap Contract Tests

**Location:** `contracts/atomic_swap/src/` and `contracts/atomic_swap/tests/`

### Enabled Test Modules

| Module | Status | Purpose | File |
|---|---|---|---|
| `cross_contract_tests` | ✅ Enabled | Integration tests calling `ip_registry` contract | `lib.rs` |
| `oracle_tests` | ✅ Enabled | Oracle integration and price validation tests | `lib.rs` |
| `benchmarks` | ✅ Enabled | Swap operation performance benchmarks | `lib.rs` (note: currently disabled at line 5451) |
| `mutation_tests` | ✅ Enabled | Mutation testing | `lib.rs` |
| `snapshot_tests` | ✅ Enabled | State snapshots for swap records | `lib.rs` |
| `upgrade_chaos_tests` | ✅ Enabled | Upgrade behavior under concurrent operations | `lib.rs` |
| `escrow_tests` | ✅ Enabled | Escrow fund handling and release logic | `lib.rs` |
| `arbitration_tests` | ✅ Enabled | Dispute resolution and arbitration flows | `lib.rs` |
| `batch_swap_features_tests` | ✅ Enabled | Batch swap execution | `lib.rs` |
| `batch_approval_tests` | ✅ Enabled | Batch swap approval workflows | `lib.rs` |
| `batch_history_tests` | ✅ Enabled | Swap history tracking for batches | `lib.rs` |
| **E2E Tests** | ✅ Enabled | End-to-end testnet integration | `tests/e2e_tests.rs` |
| **Feature Tests** | ✅ Enabled | Feature-specific scenarios | `tests/feature_tests.rs` |
| **Testnet Integration** | ✅ Enabled | Live testnet validation | `tests/testnet_integration.rs` |

### Disabled Test Modules

**All disabled modules are tracked in issue #82.**

| Module | Status | Reason | Issue | Action |
|---|---|---|---|---|
| `tests` | ❌ Disabled | Refactoring for new escrow/arbitration layer | #82 | Re-enable once escrow redesign complete |
| `prop_tests` | ❌ Disabled | Requires quickcheck/proptest update | #82 | Re-enable after test framework upgrade |
| `regression_tests` | ❌ Disabled | Accumulating obsolete test cases | #82 | Consolidate into main test suite |
| `benchmarks` (line 5451) | ❌ Disabled | Performance baseline needs recalibration | #817 | Run benchmarks and establish new baseline |
| `tests` (validation.rs) | ❌ Disabled | Validation refactor pending | #82 | Complete validation module refactoring |

**Disabled Count:** 5 test modules (tracking issue #82)

### Cleanup Plan

1. **#82 Phase 1** — Consolidate `regression_tests` into main `test` module and re-enable
2. **#817** — Recalibrate `benchmarks` baseline and re-enable
3. **#82 Phase 2** — Refactor escrow/arbitration logic, re-enable `tests` module
4. **#82 Phase 3** — Update test framework, re-enable `prop_tests`
5. **#82 Phase 4** — Complete validation module refactoring, re-enable `tests` in `validation.rs`

## API Server Tests

**Location:** `api-server/tests/`

### Enabled Test Modules

| Module | Status | Purpose |
|---|---|---|
| `integration_tests` | ✅ Enabled | Full REST API integration tests |
| `compliance_tests` | ✅ Enabled | Legal/compliance requirement validation |
| `accessibility_tests` | ✅ Enabled | WCAG accessibility checks for JSON responses |
| `cache_redis_fallback` | ✅ Enabled | Redis failover and fallback behavior |
| `cache_redis_cross_instance` | ✅ Enabled | Cache consistency across instances |

## JavaScript Layer Tests

**Location:** `src/__tests__/`

The JavaScript batch processing, matching, insurance, reputation, and royalty modules have unit tests for each subsystem. All are enabled and run in CI.

## Test Execution

### Run All Tests

```bash
# Contracts
cargo test --all

# API Server
npm test --workspaces

# Full suite
./scripts/test-all.sh
```

### Run Specific Test Suite

```bash
# IP Registry only
cargo test -p ip_registry

# Atomic Swap only
cargo test -p atomic_swap

# API Server
npm test --workspace=api-server
```

### Re-enable a Disabled Test Module

To re-enable a disabled test module (e.g., in `atomic_swap/src/lib.rs`):

1. Uncomment the `#[cfg(test)]` and `mod <name>` lines
2. Verify the test module compiles: `cargo test --no-run -p atomic_swap`
3. Run tests: `cargo test -p atomic_swap -- <module_name>::*`
4. If passing, update issue #82 with completion status
5. Commit and create a PR

## Known Issues

- **Benchmark baseline drift** — Performance benchmarks may diverge from baseline under different hardware. Recalibration recommended quarterly.
- **Cross-contract test gas limits** — Some `cross_contract_tests` push near Soroban's simulated gas ceiling; consider sharding into sub-modules if they grow further.
- **Testnet-only tests** — `testnet_integration.rs` requires a live Stellar Testnet account and XLM; skipped by default in local CI.

## References

- **Issue #82:** Master tracking for test cleanup — https://github.com/AtomicIP/AtomicIP-/issues/82
- **Issue #817:** Benchmarks refactoring — https://github.com/AtomicIP/AtomicIP-/issues/817
- **PR #823–#830:** Recent test fixes and re-enables
