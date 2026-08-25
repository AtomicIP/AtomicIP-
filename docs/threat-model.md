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
- Per-swap dispute rulings now require an **M-of-N arbitrator committee**
  (`set_arbitrator`, minimum 2-of-3 threshold, 3-of-5 or larger supported) —
  `arbitrate_dispute` requires `threshold` of the committee's `signers` to
  jointly `require_auth()` a ruling in one call; no single key can rule alone.
- A ruling is bound to submitted evidence: `arbitrate_dispute` rejects a
  ruling if `DisputeEvidence` is empty, and the evidence hashes actually
  considered (read from storage, not signer-supplied) are recorded in
  `RulingEnteredEvent` for outside audit.
- A **48-hour time-lock** (`execute_ruling`, `ProtocolConfig.arbitration_ruling_delay_secs`)
  separates ruling entry from fund release; `cancel_pending_ruling` lets the
  same committee threshold void a ruling within that window.
- All transitions are individually auditable: `ArbitratorCommitteeSetEvent`,
  `RulingEnteredEvent`, `RulingCancelledEvent`, `RulingExecutedEvent`.
- **Not covered by the above, still open**: the contract's own `Admin` role
  (`DataKey::Admin`) remains a single `Address` — it alone appoints the
  arbitrator committee and is unaffected by this mechanism. More directly,
  `resolve_dispute` still lets that single admin resolve any disputed swap
  directly, completely bypassing the committee/evidence/timelock/bond system
  described above — a real, disclosed gap, recommended as a follow-up issue
  (either remove `resolve_dispute`'s direct fund-movement path or route it
  through the same committee/timelock). `admin_rollback_swap` and
  `auto_refund_timeout` are narrower emergency/timeout escape hatches that
  remain single-admin/permissionless by design, but now correctly refund
  (never forfeit) any dispute bond in flight if they fire instead of a
  committee ruling, so they no longer orphan bonded value.
- `batch_arbitrate_swaps` — previously did not validate its caller against
  any stored arbitrator at all — is now disabled (always reverts) pending a
  follow-up migration onto the committee model. `arbitrate_swap`, a
  collateral-aware duplicate of the old single-key `arbitrate_dispute` that
  never checked swap status, is now permanently unreachable since nothing
  populates the legacy `SwapArbitrator` key anymore; this also closes a
  pre-existing double-payout risk where it could still be called after
  `resolve_dispute` had already paid out a swap.
- Admin key rotation is supported via contract upgrade path; rotation
  procedure must still be documented before mainnet.

**Status**: ⚠️ Partially mitigated — per-swap arbitration is now committee-gated,
evidence-bound, and time-locked, but the contract `Admin` role itself is still
single-key and `resolve_dispute` remains an admin-direct bypass of the new
safeguards.

---

#### 14. False Dispute Submission

**Scenario**: A party raises a dispute in bad faith — buyer disputes a valid key reveal to delay payment release, or seller disputes to stall a refund.

**Impact**: Counterparty funds locked; griefing / DoS against legitimate swap completion.

**Mitigations**:
- Disputes require an **on-chain evidence hash** (`submit_dispute_evidence`) —
  no evidence, no dispute, and (see #13) no evidence means `arbitrate_dispute`
  cannot enter a ruling at all.
- A **non-refundable dispute bond** (`MIN_DISPUTE_BOND`, the greater of 1 XLM
  or 10% of swap price) is charged on a party's first evidence submission and
  forfeited to the contract admin if the committee's ruling goes against that
  party; refunded in full if the ruling goes their way, or if the dispute is
  instead resolved via one of the non-committee paths in #13 (no ruling ⇒ no
  forfeiture).
- Disputes must be filed within `dispute_period` ledgers of the triggering event; late filings are rejected by the contract
- Repeated frivolous filings from the same address are rate-limited by the admin

**Status**: ⚠️ Partially mitigated — evidence + bond are now enforced in code
with a fixed minimum; bond forfeiture destination is the contract admin
address rather than the protocol treasury (`protocol_config().treasury`
resolves to a hardcoded placeholder today — a pre-existing, separate storage
bug, not fixed by this change).

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

---

## Anonymity Guarantees — #464 Anonymous Batch IP Commitments

### Overview

`batch_commit_ip_anonymous` lets submitters register IP commitments without
linking the transaction to a real identity. This section documents the
cryptographic model, what anonymity properties hold, and where residual risks
remain.

### How Blinded Owner Identifiers Work

The caller supplies a `blinded_owner: BytesN<32>` value instead of a real
`Address`. The recommended construction off-chain is:

```
blinded_owner = sha256(owner_address_bytes || random_nonce_32_bytes)
```

Only the `blinded_owner` hash is written on-chain — never the raw address or
nonce. An observer with access to the full ledger history cannot reverse this
hash to recover the original address without knowing the nonce.

### Anonymity Properties

| Property | Guarantee |
|---|---|
| Submitter unlinkability | The `IpRecord.owner` field is set to the contract address, not the caller. No on-chain index links the record to any `Address`. |
| Blinded-owner confidentiality | `blinded_owner` is a one-way hash; the original address + nonce pair cannot be recovered without the nonce. |
| Ownership indexing bypass | Anonymous commits intentionally skip the `OwnerIps` index, so `list_ip_by_owner` returns an empty list for any address. |
| Batch grouping resistance | Each batch must use a fresh `blinded_owner`. The replay protection prevents linking two batches to the same identity via nonce reuse. |

### Nonce-Based Replay Protection

Each `blinded_owner` value is consumed atomically on first use and stored under
`DataKey::UsedBlindedOwner(blinded_owner)`. A second call with an identical
`blinded_owner` panics with `CommitmentAlreadyRegistered` (error code 3).

This means:

- A given `blinded_owner = sha256(address || nonce)` submits **exactly one
  batch** per nonce.
- An attacker who observes the `blinded_owner` on-chain cannot replay it to
  register additional commitments under the same pseudonym.
- Submitters who need multiple batches must generate a fresh nonce for each.

### Threat Scenarios

#### 16. Blinded Owner Replay

**Scenario**: Attacker copies a `blinded_owner` from a historical transaction
and attempts to register new commitments under that identity.

**Impact**: Forged ownership linkage under another party's pseudonym.

**Mitigation**: `UsedBlindedOwner` map rejects the second call immediately.

**Status**: ✅ Mitigated

---

#### 17. Blinded Owner Brute-Force / Correlation

**Scenario**: Adversary iterates over known Stellar addresses to find a
match for an observed `blinded_owner` by computing `sha256(address || nonce)`
for each candidate.

**Impact**: De-anonymization of the submitter.

**Mitigation**:
- The nonce must be 32 bytes of cryptographically random data, making the
  search space 2^256 even if the address is known.
- Wallets **must** use a CSPRNG (e.g. `crypto.getRandomValues`) for nonce
  generation; deterministic or low-entropy nonces weaken this guarantee.

**Status**: ✅ Mitigated — provided callers use a secure nonce

---

#### 18. Traffic Analysis / Timing Correlation

**Scenario**: Adversary correlates the ledger timestamp and transaction fee
payer of an anonymous commit to a known address.

**Impact**: Partial de-anonymization via side-channel.

**Mitigation**:
- This is a **residual risk**. The protocol cannot hide the fee account on
  Stellar.
- Users requiring stronger anonymity should route submissions through an
  intermediary account (e.g. a relayer), or batch alongside other users.

**Status**: ⚠️ Residual risk — fee-account linkage is unavoidable at the
network layer

---

### Operator Recommendations

| Concern | Required Action |
|---|---|
| Nonce quality | Enforce 32-byte CSPRNG nonce in all SDKs and wallet integrations; reject user-supplied low-entropy nonces |
| Blinded owner reuse | Document that each batch requires a fresh nonce; warn if SDK detects reuse |
| Fee account exposure | Advise privacy-sensitive users to use a fresh throwaway account as the transaction fee payer |
| Audit trail | `"ip_cmt_a"` events are emitted per commitment; monitor for unusual batch sizes that may indicate Sybil behaviour |
