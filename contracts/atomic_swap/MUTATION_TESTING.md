# Mutation Testing Guide for Atomic Swap Contract

## Overview

Mutation testing is a code quality technique that evaluates test effectiveness by introducing controlled mutations (small code changes) and verifying that tests fail appropriately. This document describes the mutation testing strategy for the Atomic Swap contract.

## What is Mutation Testing?

Mutation testing works by:
1. Creating a mutant version of the code with a small, deliberate change
2. Running the test suite against the mutant
3. Checking if any test fails due to the mutation
4. If all tests pass despite the mutation, the mutation "survives" and indicates weak tests

**Survived mutations indicate areas needing better test coverage.**

## Setting Up Cargo-Mutants

### Installation

```bash
# Install cargo-mutants
cargo install cargo-mutants

# Or update to latest version
cargo install cargo-mutants --force
```

### Running Mutation Tests

```bash
# Run mutation testing on the atomic_swap contract
cd contracts/atomic_swap
cargo mutants

# Run mutation tests with specific filters
cargo mutants -p atomic_swap

# Run with verbose output to see each mutation
cargo mutants --verbose

# Run mutation tests and show the results report
cargo mutants --output csv > mutation_results.csv
```

## Mutation Testing Strategy

This contract implements a comprehensive mutation testing strategy targeting critical functions:

### 1. Status Transition Mutations

**Target Functions:** `accept_swap()`, `complete_swap()`, `cancel_swap()`

**Mutations to Test:**
- Change status values (Pending → Accepted → Completed)
- Modify status comparison operators
- Skip status update logic

**Killer Tests:**
- `mutation_killer_swap_status_pending` - Catches mutations in initial status
- `mutation_killer_swap_status_accepted` - Catches mutations in accept logic
- `mutation_killer_swap_status_completed` - Catches mutations in complete logic
- `mutation_killer_swap_status_cancelled` - Catches mutations in cancel logic

### 2. Field Integrity Mutations

**Target Functions:** All initialization and retrieval logic

**Mutations to Test:**
- Modify field values (price, token, ip_id, etc.)
- Skip field assignments
- Mix up participant assignments (seller ↔ buyer)

**Killer Tests:**
- `mutation_killer_price_field_stored_correctly` - Exact price preservation
- `mutation_killer_price_boundary_values` - Boundary conditions (1, 999_999)
- `mutation_killer_seller_preserved` - Seller identity preservation
- `mutation_killer_buyer_preserved` - Buyer identity preservation
- `mutation_killer_token_field_preserved` - Token field integrity
- `mutation_killer_ip_id_preserved` - IP ID field integrity

### 3. State Transition Logic Mutations

**Target Functions:** Validation and state check logic

**Mutations to Test:**
- Remove status checks/conditions
- Invert boolean conditions
- Change comparison operators (< → <=, > → >=)

**Killer Tests:**
- `mutation_killer_cannot_accept_when_not_pending` - Status check enforcement
- `mutation_killer_cannot_complete_when_not_accepted` - Status check enforcement
- `mutation_killer_cannot_accept_accepted_swap` (E2E) - Prevents double-accept
- `mutation_killer_cannot_complete_non_accepted_swap` (E2E) - Prevents invalid transitions

### 4. Approval Count Mutations

**Target Functions:** Approval tracking logic

**Mutations to Test:**
- Modify approval count values
- Skip approval storage
- Change approval comparison logic

**Killer Tests:**
- `mutation_killer_required_approvals_zero` - Zero approval case
- `mutation_killer_required_approvals_nonzero` - Non-zero approval values

### 5. Boolean Flag Mutations

**Target Functions:** Insurance and other boolean fields

**Mutations to Test:**
- Invert boolean values (true ↔ false)
- Skip boolean flag assignment

**Killer Tests:**
- `mutation_killer_insurance_enabled_false` - False flag preservation
- `mutation_killer_insurance_enabled_true` - True flag preservation

## Mutation Testing Coverage Goals

### Current Coverage Goals
- **Critical Functions:** 95%+ mutation kill rate
  - `initiate_swap()` - 95%+
  - `accept_swap()` - 95%+
  - `complete_swap()` - 95%+
  - `cancel_swap()` - 95%+
  - `raise_dispute()` - 95%+

- **Validation Functions:** 90%+ mutation kill rate
  - Status checks - 90%+
  - Authorization checks - 90%+
  - Field validation - 90%+

- **Overall Contract:** 85%+ mutation kill rate

### Tracking Progress

After running mutation tests, check the report for:
1. **Survived Mutations** - Mutations that didn't fail any test
2. **Equivalents** - Mutations that appear equivalent to the original
3. **Timeouts** - Mutations that cause infinite loops (should be investigated)

## Interpreting Mutation Reports

### Example Report Output
```
Mutation results summary:
  Total mutations:       156
  Mutation score:        85% (survived=23, killed=133, timeouts=0, unviable=0)
  
Highest-impact unviable mutations:
  None detected

Survived mutations by type:
  - arithmetic_operator: 5
  - boolean_operator: 3
  - comparison_operator: 15
```

### Actions for Survived Mutations

1. **Analyze the mutation** - Understand what code change survived
2. **Write a killer test** - Create a test that catches that specific mutation
3. **Add to mutation_testing_coverage.rs** - Include new killer test in the module
4. **Verify improvement** - Re-run mutation testing to confirm fix

## Common Mutation Types and Killers

| Mutation Type | Example | Killer Test Pattern |
|---|---|---|
| Status Equality | `== Pending` → `!= Pending` | Assert exact status value |
| Arithmetic | `price + fee` → `price - fee` | Assert exact calculated values |
| Comparison | `expiry > now` → `expiry >= now` | Test boundary conditions |
| Boolean Invert | `insurance = true` → `insurance = false` | Assert both true and false cases |
| Return Skip | `return value` → `return default` | Assert return values aren't lost |
| Condition Remove | `if (status == Pending)` → removed | Assert preconditions enforced |

## Best Practices

### Writing Mutation-Killable Tests

1. **Test Exact Values** - Use `assert_eq!()` instead of `assert!()`
   ```rust
   // Good: kills mutations in value assignments
   assert_eq!(swap.price, 50_i128);
   
   // Weak: might survive mutations
   assert!(swap.price > 0);
   ```

2. **Test State Transitions** - Verify before and after states
   ```rust
   // Good: catches mutations that skip state changes
   client.accept_swap(&swap_id);
   let swap = client.get_swap(&swap_id).unwrap();
   assert_eq!(swap.status, SwapStatus::Accepted);
   ```

3. **Test Boundary Conditions** - Use edge values
   ```rust
   // Good: catches off-by-one mutations
   assert_eq!(swap.required_approvals, 0);   // minimum
   assert_eq!(swap.required_approvals, 255); // maximum
   ```

4. **Test Operator Changes** - Test conditions on both sides
   ```rust
   // Good: catches comparison operator mutations
   assert!(swap.price > 0);
   assert!(swap.expiry > current_time);
   ```

5. **Test Negation** - Explicitly test false cases
   ```rust
   // Good: catches boolean inversion mutations
   assert!(!swap.insurance_enabled);
   assert!(swap.insurance_enabled);
   ```

## Continuous Integration

### CI Configuration

Add mutation testing to your CI/CD pipeline:

```yaml
# .github/workflows/mutation-tests.yml
name: Mutation Tests

on: [push, pull_request]

jobs:
  mutation-testing:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Install cargo-mutants
        run: cargo install cargo-mutants
      - name: Run mutation tests
        run: cd contracts/atomic_swap && cargo mutants -- --threads 4
      - name: Check mutation score
        run: |
          if [ $(cargo mutants 2>&1 | grep "Mutation score" | grep -o "[0-9]*") -lt 85 ]; then
            echo "Mutation score below 85%"
            exit 1
          fi
```

## Performance Optimization

Mutation testing is computationally expensive. Optimize with:

```bash
# Run in parallel (4 threads)
cargo mutants -- --threads 4

# Only test specific functions
cargo mutants -m "initiate_swap|accept_swap|complete_swap"

# Skip expensive tests
cargo mutants --exclude-target "oracle_tests" -- --threads 4
```

## Documentation and Reporting

### Generate Mutation Report

```bash
# Generate CSV report
cargo mutants -- --output csv > mutation_report.csv

# Generate JSON report
cargo mutants -- --output json > mutation_report.json
```

### Sample Report Format
```csv
status,file,line,mutation_type,mutation,original,mutated
killed,src/lib.rs,150,comparison_operator,>,>=,status > SwapStatus::Pending
survived,src/lib.rs,210,arithmetic_operator,+,-,total_swaps + 1
timeout,src/lib.rs,320,call_deletion,,get_swap(),
```

## Related Testing

- **Unit Tests** - Test individual functions
- **Integration Tests** - Test contract interactions (see `e2e_tests.rs`)
- **Invariant Tests** - Test system properties (see `invariant_tests.rs`)
- **Concurrent Tests** - Test multi-operation scenarios (see `concurrent_tests.rs`)

## Resources

- [Cargo Mutants Documentation](https://mutants.live/)
- [Mutation Testing Best Practices](https://en.wikipedia.org/wiki/Mutation_testing)
- [Soroban Test Guide](https://soroban.stellar.org/docs/learn/testing)

## Troubleshooting

### Mutation Test Timeouts

If mutations cause timeouts:
1. Investigate the mutation - may indicate infinite loop vulnerability
2. Add timeout limit: `cargo mutants -- --timeout 10`
3. Document the timeout as a known issue

### High Mutation Score Not Achievable

If unable to reach 85%+ mutation score:
1. Document unavoidable survived mutations
2. Evaluate if mutation is equivalent to original code
3. Consider test/code tradeoffs
4. Prioritize critical functions for higher coverage

### Performance Issues

If mutation testing is too slow:
1. Run on powerful CI machines (16+ cores)
2. Use `--threads` parameter for parallelization
3. Run mutation tests on PRs, not every commit
4. Use mutation test report to focus on critical functions
