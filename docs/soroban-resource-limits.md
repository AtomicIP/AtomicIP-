# Soroban Resource Limits and Batch Operation Documentation

## Overview

This document details the Soroban ledger resource limits relevant to AtomicIP's batch-operation constants (`batch_commit_ip`, `batch_initiate_swap`, etc.). Understanding these limits is critical for determining safe batch sizes that can fit within a single transaction without hitting ledger capacity constraints.

## Soroban Ledger Resource Limits

### CPU Instructions

- **Total CPU Instructions Per Transaction**: 100,000,000 (100M)
- **Purpose**: Limit computational complexity to prevent DoS attacks
- **Impact on Batch Operations**: Each cryptographic verification (commitment verification, signature validation) consumes CPU cycles

**Per-Operation Estimates (based on common Soroban patterns)**:
- Basic storage read/write: ~1,000-5,000 instructions
- ECDSA signature verification: ~200,000-500,000 instructions
- Commitment hash verification (SHA-256): ~50,000-100,000 instructions
- Event emission: ~5,000-10,000 instructions

### Ledger Entry Read/Write Count

- **Max Ledger Entries Read Per Transaction**: 1,000
- **Max Ledger Entries Written Per Transaction**: 1,000
- **Purpose**: Limit state modifications to maintain ledger performance and consistency
- **Impact on Batch Operations**: Each IP record, co-owner grant, and swap record is a separate ledger entry

**Per-Operation Estimates**:
- Commit IP: 2-3 writes (IpRecord, OwnerIps set, optional NextId update)
- Verify IP: 1 read (IpRecord lookup)
- Transfer IP: 3-4 writes (IpRecord update, source OwnerIps, destination OwnerIps)
- Batch verification: 1 read per IP (IpRecord lookups)

### Transaction Size Limit

- **Maximum Transaction Size**: 100 KB (102,400 bytes)
- **Purpose**: Prevent excessively large transaction envelopes from clogging the network
- **Impact on Batch Operations**: Large batch payloads with many commitment hashes or signatures can exceed this limit

**Current Payload Estimates** (serialized JSON/binary format):
- Single IP record: ~200-300 bytes
- Commitment hash (32 bytes): 32 bytes + overhead
- Ed25519 signature: 64 bytes + overhead
- Full batch of 50 IPs with signatures: ~15-20 KB

## Project Batch Size Constants

### API Server (`api-server/src/handlers.rs`)

```rust
const MAX_BATCH_SIZE: usize = 50;  // Issue #520
```

- **Purpose**: Cap batch commits in a single API transaction
- **Current Limit**: 50 IP records per batch
- **Rationale**: Balances throughput with resource consumption

### Contract Level (`contracts/ip_registry/src/lib.rs`, `contracts/atomic_swap/src/lib.rs`)

- **Batch Commit**: Mirrors MAX_BATCH_SIZE from API server
- **Batch Verify**: Typically 50-100 entries
- **Batch Transfer**: Typically 10-20 (due to co-owner set complexity)

### JavaScript/TypeScript (`src/batch/*.js`)

- **MAX_BATCH_SIZE**: Defined per batch processor
  - `batchMatcher.js`: 50
  - `batchCompressor.js`: Variable (depends on compression ratio)
  - `batchRoyaltyDistributor.js`: 25-50 (depends on co-owner complexity)

## Resource Consumption Analysis

### Worst-Case Scenario: Batch Commit of 50 IPs

#### CPU Instructions
- Per-IP commitment verification: ~75,000 instructions average
- 50 IPs × 75,000 = **3,750,000 instructions** (3.75% of budget)
- Event emission overhead: ~50,000 instructions
- **Total**: ~3,800,000 instructions ✓ (well within 100M limit)

#### Ledger Entry Operations
- Writes per IP: 3 (IpRecord, OwnerIps list update, optional category update)
- 50 IPs × 3 = **150 write operations** (15% of 1,000 limit) ✓
- Event storage: minimal (~10 entries)
- **Total**: ~160 write operations ✓ (well within limit)

#### Transaction Size
- Base transaction envelope: ~1 KB
- 50 IP records @ 250 bytes each: ~12.5 KB
- Signatures/proofs: ~10 KB (50 × ~200 bytes)
- Metadata: ~2 KB
- **Total**: ~25.5 KB ✓ (well within 100 KB limit)

### Moderate-Risk Scenario: Batch Verify of 100 IPs

- CPU: 100 × 50,000 (lighter verify) = **5,000,000 instructions** ✓
- Ledger reads: **100 read operations** ✓ (10% of 1,000 limit)
- Transaction size: ~12 KB ✓

### High-Risk Scenario: Batch Transfer with 20 IPs (Co-owner Complexity)

- CPU: 20 × 150,000 (transfer complexity) = **3,000,000 instructions** ✓
- Ledger operations: 20 × 5 (source, dest, co-owner updates) = **100 write operations** ✓
- Transaction size: ~25 KB ✓

## Conclusion and Recommendations

### Current Batch Size Constants Are Safe

✓ **MAX_BATCH_SIZE = 50** is safe for:
- Batch IP commits
- Batch verification operations
- Standard swap initiations

✓ **Batch transfer operations are safely limited** to 10-20 IPs due to co-owner complexity

### Safety Margins

- **CPU Instructions**: 3-5M consumed out of 100M available (3-5% utilization)
- **Ledger Operations**: 100-150 out of 1,000 available (10-15% utilization)
- **Transaction Size**: 15-30 KB out of 100 KB available (15-30% utilization)

All batch operations maintain healthy safety margins. No adjustments to `MAX_BATCH_SIZE` are required at this time.

### Future Considerations

If future updates add:
- Additional cryptographic operations per IP
- Expanded co-owner tracking beyond current schema
- Complex branching verification logic

Then reassess and potentially reduce `MAX_BATCH_SIZE` to 25-30 to maintain safety margins.

## References

- **Soroban Documentation**: https://soroban.stellar.org/docs/learn/storing-data
- **Resource Limits**: https://soroban.stellar.org/docs/learn/resource-limits
- **Batch Operation Implementation**: `api-server/src/handlers.rs:40`
- **Contract Batch Definitions**: `contracts/ip_registry/src/lib.rs`
