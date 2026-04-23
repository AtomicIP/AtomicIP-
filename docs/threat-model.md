# Threat Model for Atomic Swaps

## Overview

This document analyzes potential attack vectors in the Atomic Patent swap mechanism and documents mitigations.

## Attack Scenarios

### 1. Invalid Key Attack

**Scenario**: Seller accepts payment but reveals an invalid decryption key.

**Impact**: Buyer loses payment, seller keeps money without delivering valid IP.

**Mitigation**:
- `reveal_key` verifies the key against the stored commitment hash via `verify_commitment`
- If verification fails, transaction panics and payment remains in escrow
- Buyer can call `cancel_expired_swap` after expiry to recover funds

**Status**: ✅ Mitigated

### 2. Front-Running Attack

**Scenario**: Attacker observes a pending `reveal_key` transaction and attempts to extract the secret before it's confirmed.

**Impact**: Attacker learns the IP secret without paying.

**Mitigation**:
- Stellar's transaction ordering is deterministic within a ledger
- Secret is only revealed after payment is locked in escrow
- Once revealed, the swap completes atomically in the same transaction

**Status**: ✅ Mitigated (blockchain-level protection)

### 3. Seller Refuses to Reveal Key

**Scenario**: Buyer accepts swap and sends payment, but seller never calls `reveal_key`.

**Impact**: Buyer's funds locked indefinitely.

**Mitigation**:
- Swaps have an `expiry` timestamp (default: 7 days)
- After expiry, buyer can call `cancel_expired_swap` to recover full payment
- Seller loses reputation but cannot steal funds

**Status**: ✅ Mitigated

### 4. Duplicate Commitment Attack

**Scenario**: Attacker registers the same commitment hash multiple times to claim ownership of someone else's IP.

**Impact**: IP ownership confusion, potential fraud.

**Mitigation**:
- `commit_ip` checks `DataKey::CommitmentOwner(hash)` before registration
- Duplicate hashes are rejected with `CommitmentAlreadyRegistered` error
- Each commitment hash can only be registered once globally

**Status**: ✅ Mitigated

### 5. Non-Owner Swap Initiation

**Scenario**: Attacker initiates a swap for an IP they don't own.

**Impact**: Fraudulent sale of someone else's IP.

**Mitigation**:
- `initiate_swap` calls `registry.get_ip(ip_id)` and verifies `record.owner == seller`
- Seller must provide `require_auth()` to initiate
- Cross-contract ownership verification prevents forgery

**Status**: ✅ Mitigated

### 6. Revoked IP Swap

**Scenario**: Seller initiates swap for an IP they've already revoked.

**Impact**: Buyer purchases invalid IP.

**Mitigation**:
- `initiate_swap` checks `record.revoked` flag
- Revoked IPs cannot be swapped
- Panics with `IpIsRevoked` error

**Status**: ✅ Mitigated

### 7. Zero-Price Swap

**Scenario**: Seller creates a swap with price = 0 to transfer IP without payment tracking.

**Impact**: Off-chain deals bypass protocol fees, potential money laundering.

**Mitigation**:
- `initiate_swap` rejects `price <= 0` with `PriceMustBeGreaterThanZero` error
- All swaps must have positive price

**Status**: ✅ Mitigated

### 8. Concurrent Swap Attack

**Scenario**: Seller initiates multiple swaps for the same IP simultaneously.

**Impact**: Multiple buyers pay for the same IP.

**Mitigation**:
- `DataKey::ActiveSwap(ip_id)` tracks active swaps per IP
- Second `initiate_swap` for same IP is rejected with `ActiveSwapAlreadyExistsForThisIpId`
- Lock released only when swap reaches `Completed` or `Cancelled`

**Status**: ✅ Mitigated

### 9. Replay Attack

**Scenario**: Attacker replays a previous `reveal_key` transaction to complete a different swap.

**Impact**: Unauthorized swap completion.

**Mitigation**:
- Each swap has a unique `swap_id`
- `reveal_key` verifies the secret against the specific IP's commitment hash
- Stellar's transaction sequence numbers prevent replay across ledgers

**Status**: ✅ Mitigated (blockchain-level protection)

### 10. Payment Token Manipulation

**Scenario**: Buyer uses a malicious token contract that doesn't actually transfer funds.

**Impact**: Seller reveals key but receives no payment.

**Mitigation**:
- Seller chooses the token contract address when initiating swap
- Seller should only accept well-known tokens (XLM, USDC, EURC)
- Wallet UIs should warn sellers about unknown tokens

**Status**: ⚠️ Partially mitigated (requires off-chain verification)

### 11. Commitment Brute-Force

**Scenario**: Attacker attempts to brute-force the secret from the commitment hash.

**Impact**: IP secret revealed without payment.

**Mitigation**:
- Pedersen commitment scheme uses SHA-256 with blinding factor
- Blinding factor makes brute-force computationally infeasible (2^256 search space)
- Users must generate cryptographically random blinding factors

**Status**: ✅ Mitigated (cryptographic security)

### 12. Storage Expiry Attack

**Scenario**: Attacker waits for IP record TTL to expire, then registers the same commitment.

**Impact**: IP ownership stolen after expiry.

**Mitigation**:
- All persistent storage uses `LEDGER_BUMP = 6_307_200` (~1 year)
- Every read/write extends TTL automatically
- Active IPs remain valid indefinitely through normal usage

**Status**: ✅ Mitigated

## Dispute Resolution Threat Model

### Overview

The dispute resolution feature allows a designated admin (or multi-sig admin group) to adjudicate contested swaps — for example, when a buyer claims the revealed key is invalid despite on-chain verification passing, or when off-chain delivery of associated materials is disputed.

---

### 13. Admin Collusion

**Scenario**: A single admin account is compromised or acts maliciously, ruling in favour of one party to steal funds or IP.

**Impact**: Fraudulent dispute outcomes; loss of buyer funds or seller IP rights.

**Mitigation**:
- Admin role must be held by a **multi-sig account** (M-of-N threshold, recommended 2-of-3 minimum)
- All admin actions are recorded on-chain with the admin's address and ledger timestamp
- A time-lock delay (e.g. 48 hours) between dispute ruling and fund release gives parties time to escalate
- Governance upgrade path: admin key can be rotated via on-chain vote in future versions

**Operator Recommendation**: Deploy with a multi-sig admin from day one. Never use a single EOA as the dispute admin on mainnet.

**Status**: ⚠️ Partially mitigated (requires operator configuration)

---

### 14. False Dispute Submission

**Scenario**: A buyer or seller raises a dispute in bad faith — e.g. a buyer disputes a valid key reveal to delay payment release, or a seller disputes to stall a refund.

**Impact**: Legitimate counterparty funds locked; griefing / denial-of-service on the swap.

**Mitigation**:
- Disputes require **on-chain evidence submission** (e.g. a hash of supporting material stored via `dispute_evidence` field)
- A non-refundable **dispute bond** is required to open a dispute; bond is forfeited if the dispute is ruled frivolous
- Dispute window is bounded: disputes must be raised within `dispute_period` ledgers after the triggering event
- Repeated frivolous disputes from the same address can be flagged and rate-limited by the admin

**Operator Recommendation**: Set a meaningful dispute bond (e.g. 10% of swap price, minimum 1 XLM) to deter bad-faith filings. Publish the evidence-submission format in your integration guide.

**Status**: ⚠️ Partially mitigated (requires operator configuration)

---

### 15. Timeout Abuse

**Scenario**: A party deliberately stalls the dispute process — e.g. admin never rules, or a party never submits evidence — to keep funds locked indefinitely or force the other party to abandon their claim.

**Impact**: Funds locked beyond the intended swap expiry; effective denial of refund or payment.

**Mitigation**:
- **Auto-resolution on timeout**: if the admin has not ruled within `dispute_timeout` ledgers, the contract automatically resolves in favour of the buyer (refund) as the safe default
- Evidence submission deadline is enforced on-chain; failure to submit evidence within the window is treated as conceding the dispute
- All timeout thresholds are set at contract initialisation and cannot be changed without a contract upgrade (preventing admin from extending timeouts to stall)

**Operator Recommendation**: Set `dispute_timeout` conservatively (e.g. 14 days / ~120,960 ledgers). Document the auto-resolution default clearly so both parties understand the incentive to act promptly.

**Status**: ✅ Mitigated (when timeout parameters are correctly configured)

---

### Dispute Resolution: Operator Recommendations Summary

| Concern | Recommended Configuration |
|---|---|
| Admin collusion | Multi-sig admin, minimum 2-of-3 |
| False disputes | Require evidence hash on submission; set dispute bond ≥ 1 XLM |
| Timeout abuse | Set `dispute_timeout` ≤ 14 days; auto-resolve to buyer refund |
| Audit trail | Log all dispute events (open, evidence, ruling, auto-resolve) on-chain |
| Key rotation | Plan admin key rotation procedure before mainnet launch |

---

## Unmitigated Risks

### Off-Chain Secret Loss

**Risk**: User loses their `secret` and `blinding_factor`.

**Impact**: Cannot prove IP ownership or complete swaps.

**Recommendation**: Wallets should implement encrypted backup and recovery mechanisms.

### Legal Enforceability

**Risk**: On-chain IP commitment may not be recognized in all jurisdictions.

**Impact**: Limited legal protection in some countries.

**Recommendation**: Users should consult local IP attorneys for jurisdiction-specific advice.

### Oracle Problem

**Risk**: No on-chain mechanism to verify the quality or validity of the IP itself.

**Impact**: Buyer may purchase worthless or invalid IP.

**Recommendation**: Buyers should conduct off-chain due diligence before accepting swaps.

## Security Best Practices

For wallet providers:
- Encrypt all stored secrets with user's master password
- Generate blinding factors using `crypto.getRandomValues()` or equivalent
- Warn users before revealing keys in swaps
- Display swap expiry times prominently
- Implement transaction simulation before submission

For users:
- Backup secrets in multiple secure locations
- Only accept swaps for IPs you've verified off-chain
- Use well-known token contracts (XLM, USDC)
- Monitor swap expiry times

## Audit Status

- Internal security review: ✅ Complete
- External audit: ⏳ Pending
- Bug bounty program: Planned for v2.0

## Reporting Vulnerabilities

See [SECURITY.md](../SECURITY.md) for responsible disclosure process.
