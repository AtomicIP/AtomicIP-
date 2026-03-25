# Bugfix Requirements Document

## Introduction

In `commit_ip`, the function accepts an `owner` parameter and calls `owner.require_auth()`.
In Soroban's auth model, `require_auth()` checks that the address authorized the current
contract invocation — but the address itself is supplied by the caller. This means a caller
who can satisfy auth for *any* address (e.g. a contract address they control, or in a
test/mock environment) can register IP under an arbitrary owner identity.

The fix must ensure the auth model is correctly enforced and is explicitly tested, so that
no caller can commit IP on behalf of an address they do not legitimately control.

## Bug Analysis

### Current Behavior (Defect)

1.1 WHEN `commit_ip` is called with an `owner` address that is not the transaction invoker THEN the system accepts the call as long as `owner.require_auth()` does not panic, allowing IP to be registered under an arbitrary owner address.

1.2 WHEN a malicious caller supplies a third-party address as `owner` and satisfies the auth check through a forged or contract-controlled authorization THEN the system records the IP as belonging to that third-party address without their consent.

### Expected Behavior (Correct)

2.1 WHEN `commit_ip` is called with an `owner` address that does not match the transaction invoker THEN the system SHALL reject the call with an authorization error.

2.2 WHEN `commit_ip` is called and the `owner` address matches the transaction invoker and auth is satisfied THEN the system SHALL register the IP record under that owner address and return a valid IP ID.

### Unchanged Behavior (Regression Prevention)

3.1 WHEN a legitimate owner calls `commit_ip` with their own address as `owner` and provides a valid commitment hash THEN the system SHALL CONTINUE TO store the IP record with the correct owner, commitment hash, and ledger timestamp.

3.2 WHEN `get_ip` is called with a valid IP ID THEN the system SHALL CONTINUE TO return the correct `IpRecord` containing owner, commitment hash, and timestamp.

3.3 WHEN `list_ip_by_owner` is called for an owner who has committed IPs THEN the system SHALL CONTINUE TO return all IP IDs associated with that owner.

3.4 WHEN `verify_commitment` is called with a matching secret THEN the system SHALL CONTINUE TO return `true`, and `false` for a non-matching secret.
