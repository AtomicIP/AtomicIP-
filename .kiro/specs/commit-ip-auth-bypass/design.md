# commit-ip-auth-bypass Bugfix Design

## Overview

`commit_ip` in `contracts/ip_registry/src/lib.rs` accepts a caller-supplied `owner: Address`
and calls `owner.require_auth()`. In Soroban's auth model, `require_auth()` verifies that the
given address authorized the current contract invocation — but it does NOT verify that the
address is the transaction invoker. A caller who controls a contract address (or operates in a
test/mock environment with relaxed auth) can pass any address as `owner`, satisfy the auth
check, and register IP under an identity they do not legitimately control.

The fix removes the `owner` parameter from the public interface and derives the owner from
`env.current_contract_address()` / the invoker, so the auth check is always against the
actual caller.

## Glossary

- **Bug_Condition (C)**: The condition where `owner` supplied to `commit_ip` differs from the
  actual transaction invoker, allowing IP to be registered under an arbitrary address.
- **Property (P)**: The desired behavior — `commit_ip` SHALL only register IP under the address
  that is the authenticated invoker of the call.
- **Preservation**: All read-only functions (`get_ip`, `list_ip_by_owner`, `verify_commitment`)
  and the happy-path write behavior for legitimate callers must remain unchanged.
- **`commit_ip`**: The function in `contracts/ip_registry/src/lib.rs` that timestamps a new IP
  commitment and stores an `IpRecord`.
- **`require_auth()`**: Soroban SDK method that asserts the address authorized the current
  invocation. It does NOT assert the address equals the invoker.
- **invoker**: The address that signed and submitted the transaction calling `commit_ip`.

## Bug Details

### Bug Condition

The bug manifests when `commit_ip` is called with an `owner` address that is not the
transaction invoker. Because `require_auth()` only checks that the supplied address authorized
the invocation (not that it IS the invoker), a caller can forge ownership by passing a
third-party address they can authorize through a contract they control.

**Formal Specification:**
```
FUNCTION isBugCondition(invoker, owner)
  INPUT: invoker of type Address  -- the actual transaction signer
         owner   of type Address  -- the address passed as parameter
  OUTPUT: boolean

  RETURN invoker != owner
END FUNCTION
```

### Examples

- Alice calls `commit_ip(bob_address, hash)` from her wallet. `bob_address.require_auth()`
  passes because Alice controls a proxy contract that satisfies auth for Bob. IP is recorded
  under Bob without Bob's consent. **Expected**: call rejected with auth error.
- A test harness calls `commit_ip(arbitrary_address, hash)` with mock auth enabled. The call
  succeeds and registers IP under `arbitrary_address`. **Expected**: call rejected.
- Alice calls `commit_ip(alice_address, hash)` — owner matches invoker, auth satisfied.
  **Expected**: IP registered correctly under Alice. (This is the non-buggy path.)

## Expected Behavior

### Preservation Requirements

**Unchanged Behaviors:**
- A legitimate owner calling `commit_ip` with their own address (post-fix: no `owner` param,
  derived from invoker) and a valid commitment hash SHALL continue to store the `IpRecord`
  with the correct owner, commitment hash, and ledger timestamp.
- `get_ip` called with a valid IP ID SHALL continue to return the correct `IpRecord`.
- `list_ip_by_owner` called for an owner with committed IPs SHALL continue to return all
  associated IP IDs.
- `verify_commitment` called with a matching secret SHALL continue to return `true`; `false`
  for a non-matching secret.

**Scope:**
All inputs that do NOT involve a mismatched `owner` / invoker pair are unaffected. This
includes:
- Legitimate `commit_ip` calls where the caller is the owner
- All read-only contract functions
- The `AtomicSwap` contract (separate crate, no changes required)

## Hypothesized Root Cause

1. **Caller-Controlled `owner` Parameter**: `commit_ip` accepts `owner: Address` as a
   parameter. The caller fully controls this value. `require_auth()` on a caller-supplied
   address does not bind the address to the actual invoker identity.

2. **Misunderstanding of `require_auth()` Semantics**: `require_auth()` checks that the
   address approved the invocation (via signature or sub-auth), not that it equals the
   invoker. In a contract-to-contract call, a malicious intermediary contract can satisfy
   auth for any address it controls.

3. **No Invoker Binding**: The function never calls `env.current_contract_address()` or any
   SDK primitive that returns the actual invoker, so there is no mechanism to compare the
   supplied `owner` against the real caller.

## Correctness Properties

Property 1: Bug Condition - Mismatched Owner Rejected

_For any_ call to `commit_ip` where the supplied `owner` address does not equal the
transaction invoker, the fixed function SHALL reject the call with an authorization error
and SHALL NOT store any `IpRecord`.

**Validates: Requirements 2.1**

Property 2: Preservation - Legitimate Owner Succeeds

_For any_ call to `commit_ip` where the invoker is the legitimate owner and auth is
satisfied, the fixed function SHALL register the IP record under that owner address,
store the correct commitment hash and ledger timestamp, and return a valid IP ID —
identical behavior to the original function on this input class.

**Validates: Requirements 2.2, 3.1**

Property 3: Preservation - Read-Only Functions Unaffected

_For any_ call to `get_ip`, `list_ip_by_owner`, or `verify_commitment`, the fixed
contract SHALL produce exactly the same result as the original contract, preserving
all read behavior.

**Validates: Requirements 3.2, 3.3, 3.4**

## Fix Implementation

### Changes Required

**File**: `contracts/ip_registry/src/lib.rs`

**Function**: `commit_ip`

**Specific Changes**:

1. **Remove `owner` parameter**: Change the signature from
   `commit_ip(env: Env, owner: Address, commitment_hash: BytesN<32>)` to
   `commit_ip(env: Env, commitment_hash: BytesN<32>)`.

2. **Derive owner from invoker**: Obtain the caller's address via
   `let owner = env.current_contract_address();` — or more precisely, use the Soroban
   pattern of requiring auth on the invoker directly. The idiomatic approach is to accept
   no `owner` param and use `env.current_contract_address()` only for the contract itself;
   for user-facing calls the invoker is obtained by requiring auth on a known address.
   The correct Soroban pattern: remove the param, call `env.current_contract_address()` is
   wrong for user wallets. The correct fix is to keep `owner` but add an explicit check:
   ```rust
   let invoker = env.current_contract_address(); // NOT correct for user wallets
   ```
   The proper Soroban idiom for "caller must be owner" is:
   ```rust
   // Option A (recommended): remove owner param, derive from auth context
   // Soroban does not expose a direct "get invoker" API in the same way as classic.
   // The idiomatic fix is to keep owner as param but assert it equals the invoker
   // by using require_auth AND verifying via a stored/expected value, OR
   // Option B: keep owner param, call owner.require_auth(), and document that
   // the SDK guarantees the address authorized THIS specific invocation.
   ```
   After reviewing Soroban SDK docs: `require_auth()` IS the correct mechanism. The
   vulnerability exists only if auth can be forged. The fix is to add documentation
   and an explicit test confirming the SDK enforces this correctly, per user task 1 and 3.

3. **Add explicit non-owner test** (user task 2): Add a test that calls `commit_ip` with
   a different address as `owner` and asserts the call panics/fails with an auth error.

4. **Document auth model** (user task 3): Add inline doc comments to `commit_ip` explaining
   that `require_auth()` ensures only the address itself (or an authorized sub-invoker) can
   call this function, and that the Soroban runtime enforces this at the protocol level.

5. **Test file**: Add or extend `contracts/ip_registry/src/lib.rs` test module with:
   - A test for the non-owner rejection case
   - A test confirming the happy path still works

### Auth Model Clarification

In Soroban, `address.require_auth()` is enforced by the host environment — it cannot be
bypassed by a caller unless they genuinely hold authorization for that address (private key
or delegated sub-auth). The bug as described ("forge auth") is only possible in:
- Test environments using `env.mock_all_auths()` (intentional test helper)
- Contract-to-contract calls where the intermediary contract holds legitimate auth

The fix therefore focuses on: (a) confirming the SDK model is correctly used, (b) adding
an explicit test that demonstrates rejection without mocked auth, and (c) documenting the
auth model clearly.

## Testing Strategy

### Validation Approach

Two-phase approach: first run exploratory tests on the unfixed code to surface any
counterexamples, then verify the fix (documentation + explicit test) satisfies all
correctness properties.

### Exploratory Bug Condition Checking

**Goal**: Demonstrate that without `mock_all_auths`, a non-owner cannot call `commit_ip`
on behalf of another address. Confirm the SDK enforces auth correctly.

**Test Plan**: Write a test that creates two addresses (`alice`, `bob`), mocks auth only
for `alice`, then calls `commit_ip(bob, hash)`. Assert the call panics.

**Test Cases**:
1. **Non-owner call without mock**: Call `commit_ip(bob_address, hash)` with only Alice's
   auth mocked — expect panic (will pass on unfixed code if SDK is correct, confirming
   the bug is only exploitable via `mock_all_auths` misuse).
2. **Non-owner call with `mock_all_auths`**: Call `commit_ip(bob_address, hash)` with
   `mock_all_auths()` — expect success, demonstrating the test-env attack surface.
3. **Owner call**: Call `commit_ip(alice_address, hash)` with Alice's auth — expect success.

**Expected Counterexamples**:
- If test 1 succeeds (does not panic), the SDK is not enforcing auth — root cause confirmed.
- If test 1 panics as expected, the SDK is correct and the risk is limited to test misuse.

### Fix Checking

**Goal**: Verify that for all inputs where the bug condition holds, the fixed function
produces the expected behavior (rejection).

**Pseudocode:**
```
FOR ALL (invoker, owner) WHERE isBugCondition(invoker, owner) DO
  result := commit_ip_fixed(env_with_auth_for(invoker), owner, hash)
  ASSERT result == AuthError (panic)
END FOR
```

### Preservation Checking

**Goal**: Verify that for all inputs where the bug condition does NOT hold, behavior is
identical before and after the fix.

**Pseudocode:**
```
FOR ALL (invoker, owner, hash) WHERE NOT isBugCondition(invoker, owner) DO
  ASSERT commit_ip_original(env, owner, hash) == commit_ip_fixed(env, owner, hash)
END FOR
```

**Testing Approach**: Property-based testing generates many (address, hash) pairs where
`owner == invoker` and asserts the stored record is always correct.

**Test Cases**:
1. **Happy-path preservation**: For any valid `(owner, hash)` where owner is the invoker,
   the returned IP ID is valid and `get_ip(id)` returns the correct record.
2. **Index preservation**: After a legitimate commit, `list_ip_by_owner(owner)` contains
   the new ID.
3. **Verify commitment preservation**: `verify_commitment(id, hash)` returns `true` for
   the committed hash and `false` for any other value.

### Unit Tests

- Test that `commit_ip` panics when called with a non-owner address (no `mock_all_auths`)
- Test that `commit_ip` succeeds for the legitimate owner and stores the correct record
- Test `get_ip` returns the correct `IpRecord` after a commit
- Test `list_ip_by_owner` returns all IDs for an owner with multiple commits
- Test `verify_commitment` returns `true` for matching hash, `false` otherwise

### Property-Based Tests

- Generate random `BytesN<32>` commitment hashes; verify each commit stores the exact hash
- Generate multiple commits for the same owner; verify `list_ip_by_owner` returns all IDs
  in order
- Generate random non-matching secrets; verify `verify_commitment` always returns `false`

### Integration Tests

- Full flow: commit IP as legitimate owner → get IP → verify commitment → list by owner
- Confirm that after the fix, the non-owner rejection is consistent across all test modes
  except `mock_all_auths`
- Confirm `AtomicSwap` contract is unaffected (no shared state or auth dependency)
