# Contract Invariants

This document defines the invariants that must hold for the Atomic Patent smart contracts.

## IP Registry Invariants

### I1: Commitment Uniqueness
- **Definition**: Each IP commitment hash must be unique per owner within a time window.
- **Enforcement**: The contract rejects duplicate commitments from the same owner within 24 hours.
- **Verification**: `verify_commitment_uniqueness(owner, hash) -> bool`

### I2: Timestamp Monotonicity
- **Definition**: IP record timestamps must be monotonically increasing.
- **Enforcement**: New commitments cannot have timestamps earlier than existing records.
- **Verification**: `verify_timestamp_order(ip_id_1, ip_id_2) -> bool`

### I3: Owner Consistency
- **Definition**: An IP record's owner cannot change after creation.
- **Enforcement**: The contract stores owner immutably at commitment time.
- **Verification**: `verify_owner_immutability(ip_id, owner) -> bool`

### I4: Commitment Verification
- **Definition**: A commitment can only be verified with the correct secret.
- **Enforcement**: Pedersen commitment verification uses cryptographic proof.
- **Verification**: `verify_commitment_correctness(ip_id, secret) -> bool`

## Atomic Swap Invariants

### S1: Fee Accounting
- **Definition**: Total fees collected = sum of all swap fees.
- **Enforcement**: Each swap records its fee; total is auditable.
- **Verification**: `verify_total_fees(swaps: Vec<SwapRecord>) -> bool`

### S2: Payment Atomicity
- **Definition**: Payment and key reveal must occur together or not at all.
- **Enforcement**: Escrow holds payment until key is revealed; refund if timeout.
- **Verification**: `verify_payment_key_atomicity(swap_id) -> bool`

### S3: Swap State Transitions
- **Definition**: Swaps follow valid state transitions: Pending → Active → Completed/Cancelled.
- **Enforcement**: State machine validates transitions.
- **Verification**: `verify_state_transition(swap_id, from_state, to_state) -> bool`

### S4: Escrow Balance
- **Definition**: Escrow balance = sum of all pending swap payments.
- **Enforcement**: Escrow is updated atomically with swap state changes.
- **Verification**: `verify_escrow_balance(total_pending_payments) -> bool`

### S5: Key Validity
- **Definition**: A revealed key must decrypt the commitment hash correctly.
- **Enforcement**: Key validation happens before payment release.
- **Verification**: `verify_key_validity(swap_id, key) -> bool`

## Testing Strategy

### Invariant Checks After Each Operation

1. **After `commit_ip`**: Verify I1, I2, I3
2. **After `verify_commitment`**: Verify I4
3. **After `initiate_swap`**: Verify S1, S3
4. **After `accept_swap`**: Verify S2, S4
5. **After `reveal_key`**: Verify S5, S2
6. **After `cancel_swap`**: Verify S3, S4

### Property-Based Testing

Use fuzzing to verify invariants hold under random sequences of operations:
- Generate random commitments and swaps
- Execute operations in random order
- Check all invariants after each operation
- Report violations with minimal reproducer

## Monitoring

Invariant violations should trigger:
1. Alert to operations team
2. Contract pause (if critical)
3. Forensic analysis of transaction history
4. Potential rollback to last known good state
