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

## Dispute Resolution

### Overview

The dispute resolution mechanism allows a designated admin to adjudicate contested swaps where on-chain verification alone is insufficient — e.g. a buyer claims the revealed key does not decrypt the promised IP, or off-chain delivery of associated materials is disputed. The admin can rule in favour of either party, triggering fund release or refund. Because this introduces a privileged role, it is the highest-risk surface in the protocol and requires careful operational controls.

---

### Attack Vectors

#### 13. Admin Collusion

**Scenario**: A single admin account is compromised or acts maliciously, ruling in favour of one party to steal funds or IP rights.

**Impact**: Fraudulent dispute outcomes; direct loss of buyer funds or seller IP.

**Mitigations**:
- Admin role must be a **multi-sig account** (minimum 2-of-3 threshold; 3-of-5 recommended for high-value deployments)
- All admin rulings are recorded on-chain with the admin address and ledger timestamp — fully auditable
- A **48-hour time-lock** between ruling and fund release gives the losing party time to escalate off-chain
- Admin key rotation is supported via contract upgrade path; rotation procedure must be documented before mainnet

**Status**: ⚠️ Partially mitigated — depends on operator deploying multi-sig correctly

---

#### 14. False Dispute Submission

**Scenario**: A party raises a dispute in bad faith — buyer disputes a valid key reveal to delay payment release, or seller disputes to stall a refund.

**Impact**: Counterparty funds locked; griefing / DoS against legitimate swap completion.

**Mitigations**:
- Disputes require an **on-chain evidence hash** (`dispute_evidence` field) submitted at filing time — no evidence, no dispute
- A **non-refundable dispute bond** (minimum 1 XLM or 10% of swap price, whichever is greater) is forfeited if the dispute is ruled frivolous
- Disputes must be filed within `dispute_period` ledgers of the triggering event; late filings are rejected by the contract
- Repeated frivolous filings from the same address are rate-limited by the admin

**Status**: ⚠️ Partially mitigated — bond amount and evidence format must be configured by operator

---

#### 15. Timeout Abuse

**Scenario**: A party deliberately stalls — admin never rules, or a party withholds evidence — to keep funds locked indefinitely or force the counterparty to abandon their claim.

**Impact**: Funds locked beyond intended swap expiry; effective denial of refund or payment.

**Mitigations**:
- **Auto-resolution on timeout**: if the admin has not ruled within `dispute_timeout` ledgers, the contract automatically refunds the buyer as the safe default
- Evidence submission deadline is enforced on-chain; failure to submit within the window is treated as conceding the dispute
- `dispute_timeout` is set at contract initialisation and is **immutable** — admin cannot extend it to stall resolution

**Status**: ✅ Mitigated — provided `dispute_timeout` is set correctly at deploy time

---

### Residual Risks

| Risk | Detail |
|---|---|
| Off-chain evidence integrity | The contract stores only a hash of evidence; the underlying data lives off-chain and could be lost or withheld. Operators should require evidence to be pinned to a content-addressed store (e.g. IPFS). |
| Admin key compromise post-ruling | If the admin key is compromised after a ruling but before the time-lock expires, an attacker could attempt to reverse the ruling. Multi-sig and time-lock together reduce but do not eliminate this window. |
| Governance capture | In future versions where admin is controlled by token vote, a majority token holder could capture dispute outcomes. Quorum and time-lock requirements must be enforced at the governance layer. |

---

### Operator Recommendations

| Concern | Required Action |
|---|---|
| Admin collusion | Deploy with multi-sig admin (2-of-3 minimum); **never** use a single EOA on mainnet |
| False disputes | Require evidence hash at submission; set bond ≥ max(1 XLM, 10% of swap price) |
| Timeout abuse | Set `dispute_timeout` ≤ 14 days (~120,960 ledgers); verify auto-resolve defaults to buyer refund |
| Audit trail | Emit on-chain events for all state transitions: `DisputeOpened`, `EvidenceSubmitted`, `DisputeRuled`, `DisputeAutoResolved` |
| Key rotation | Define and test admin key rotation procedure before mainnet launch; store rotation policy in governance docs |
| Evidence availability | Require disputing parties to pin evidence to IPFS or equivalent; store CID in `dispute_evidence` field |

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
