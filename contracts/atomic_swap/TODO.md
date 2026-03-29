# Protocol Fee Implementation TODO
Status: Approved plan - implementing step-by-step.

## Steps:
- [ ] Step 1: Add ProtocolConfig struct, DataKey, ProtocolFeeEvent, and set_protocol_config function to src/lib.rs.
- [ ] Step 2: Modify reveal_key in src/lib.rs to deduct fee and transfer to treasury/seller.
- [ ] Step 3: Add new tests to src/lib.rs for config, fee deduction, edges.
- [ ] Step 4: Update existing payment test in src/lib.rs to verify fee.
- [ ] Step 5: Run `cd contracts/atomic_swap && cargo test` to verify.
- [ ] Step 6: Update this TODO.md with completion.

Next: Step 3 (tests). Core logic complete: structs, config fn, reveal_key fee logic implemented. Tests pending.
