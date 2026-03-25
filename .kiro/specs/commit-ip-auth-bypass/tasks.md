# Implementation Plan

- [x] 1. Write bug condition exploration test
  - **Property 1: Bug Condition** - Non-Owner Commit Rejected by SDK
  - **CRITICAL**: This test MUST FAIL on unfixed code (or surface the attack surface) — failure/panic confirms the bug condition
  - **DO NOT attempt to fix the test or the code when it fails**
  - **NOTE**: This test encodes the expected behavior — it will validate the fix when it passes after implementation
  - **GOAL**: Surface counterexamples that demonstrate a non-owner can commit IP on behalf of another address
  - **Scoped PBT Approach**: Scope the property to the concrete failing case — `commit_ip(bob_address, hash)` called with only Alice's auth mocked
  - In `contracts/ip_registry/src/lib.rs` test module, add a test `test_non_owner_cannot_commit`:
    - Create `env`, `alice`, `bob` addresses
    - Mock auth only for `alice` (NOT `mock_all_auths`)
    - Call `commit_ip(bob_address, hash)` — assert the call panics with an auth error
    - This confirms `isBugCondition(alice, bob)` is true: invoker != owner
  - Also add a second variant using `mock_all_auths()` to document the test-env attack surface
  - Run test on UNFIXED code
  - **EXPECTED OUTCOME**: Test with selective mock PANICS (SDK enforces auth); test with `mock_all_auths` SUCCEEDS (documents the risk)
  - Document counterexamples found to understand root cause
  - Mark task complete when test is written, run, and outcome is documented
  - _Requirements: 1.1, 1.2_

- [ ] 2. Write preservation property tests (BEFORE implementing fix)
  - **Property 2: Preservation** - Legitimate Owner Commit and Read Functions Unaffected
  - **IMPORTANT**: Follow observation-first methodology
  - Observe: `commit_ip(alice, hash)` with Alice's auth mocked returns a valid `u64` IP ID on unfixed code
  - Observe: `get_ip(id)` returns `IpRecord { owner: alice, commitment_hash: hash, timestamp: _ }`
  - Observe: `list_ip_by_owner(alice)` returns `[id]` after one commit
  - Observe: `verify_commitment(id, hash)` returns `true`; `verify_commitment(id, other_hash)` returns `false`
  - Write property-based tests in the test module:
    - For any valid `BytesN<32>` commitment hash, `commit_ip(owner, hash)` where owner == invoker stores the exact hash and returns a valid ID
    - For multiple commits by the same owner, `list_ip_by_owner` returns all IDs in insertion order
    - For any non-matching secret, `verify_commitment` returns `false`
  - Verify all tests PASS on UNFIXED code (confirms baseline behavior to preserve)
  - _Requirements: 2.2, 3.1, 3.2, 3.3, 3.4_

- [ ] 3. Fix for commit-ip-auth-bypass

  - [ ] 3.1 Verify auth model is correctly enforced by Soroban SDK
    - Review `commit_ip` in `contracts/ip_registry/src/lib.rs`
    - Confirm `owner.require_auth()` is the correct Soroban idiom for "caller must be this address"
    - Confirm the SDK host enforces this at the protocol level (cannot be bypassed outside `mock_all_auths`)
    - Document findings inline: the vulnerability is limited to test environments using `mock_all_auths()`
    - _Bug_Condition: isBugCondition(invoker, owner) where invoker != owner_
    - _Expected_Behavior: commit_ip rejects calls where owner != invoker with an auth panic_
    - _Preservation: All read-only functions and legitimate owner commits remain unchanged_
    - _Requirements: 1.1, 1.2, 2.1_

  - [ ] 3.2 Add inline documentation to `commit_ip`
    - Add doc comment to `commit_ip` explaining the auth model:
      - `owner.require_auth()` ensures only the `owner` address (or a delegated sub-invoker) can call this function
      - The Soroban runtime enforces this at the protocol level; it cannot be bypassed by a caller who does not hold authorization for `owner`
      - The only exception is test environments using `env.mock_all_auths()`, which is an intentional test helper
    - _Requirements: 2.1, 2.2_

  - [ ] 3.3 Verify bug condition exploration test now passes
    - **Property 1: Expected Behavior** - Non-Owner Commit Rejected by SDK
    - **IMPORTANT**: Re-run the SAME test from task 1 — do NOT write a new test
    - The test from task 1 encodes the expected behavior (non-owner call panics)
    - When this test passes, it confirms the SDK auth enforcement is correctly validated
    - Run `test_non_owner_cannot_commit` from step 1
    - **EXPECTED OUTCOME**: Test PASSES (confirms bug condition is correctly rejected by the SDK)
    - _Requirements: 2.1_

  - [ ] 3.4 Verify preservation tests still pass
    - **Property 2: Preservation** - Legitimate Owner Commit and Read Functions Unaffected
    - **IMPORTANT**: Re-run the SAME tests from task 2 — do NOT write new tests
    - Run all preservation property tests from step 2
    - **EXPECTED OUTCOME**: Tests PASS (confirms no regressions in happy-path and read-only behavior)
    - Confirm all tests still pass after documentation changes (no regressions)

- [ ] 4. Checkpoint - Ensure all tests pass
  - Run `cargo test` in `contracts/ip_registry/`
  - Ensure all tests pass; ask the user if questions arise
