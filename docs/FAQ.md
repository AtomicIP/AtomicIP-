# Frequently Asked Questions (FAQ)

> Closes #1061. This FAQ collects the questions most frequently raised in issues,
> PR reviews and support channels. Each answer is intentionally short and links to
> the authoritative document for detail.

**Last reviewed:** 2026-09-26 · **Owner:** Docs maintainers · **Review cadence:** monthly (see [Maintaining this FAQ](#maintaining-this-faq))

## Contents

- [General](#general)
- [IP Registry & Commitments](#ip-registry--commitments)
- [Atomic Swaps](#atomic-swaps)
- [Disputes](#disputes)
- [API Server](#api-server)
- [Build, Deploy & Operations](#build-deploy--operations)
- [Troubleshooting Error Codes](#troubleshooting-error-codes)
- [Maintaining this FAQ](#maintaining-this-faq)

---

## General

### What is Atomic Patent?
A decentralized IP registry on Stellar Soroban. You commit a hash of your design to
prove prior art without revealing it, and sell it via an atomic swap where payment
and decryption key are exchanged in one transaction.
→ [README](../README.md), [Architecture](architecture.md)

### Which contracts make up the system?
- `contracts/ip_registry` — commits, verifies, transfers and revokes IP records.
- `contracts/atomic_swap` — swaps, escrow, disputes, arbitration, auctions, insurance.

The `api-server` crate is an optional REST/GraphQL gateway in front of both.
→ [Architecture](architecture.md)

### Does committing an IP make it public?
No. Only the commitment hash (a Pedersen-style commitment) is stored on-chain. The
underlying content stays private until you reveal it (e.g. during a swap).
→ [Commitment Scheme](commitment-scheme.md)

---

## IP Registry & Commitments

### How do I prove I had an idea at a certain time?
Call `commit_ip` with your commitment hash. The ledger timestamp stored on the
`IpRecord` is your proof. Later, `verify_commitment` checks that a secret + blinding
factor opens that commitment.
→ [Commitment Scheme](commitment-scheme.md), [API Reference](api-reference.md)

### I lost my blinding factor. Can I recover it?
No. The blinding factor is never stored on-chain; without it you cannot open the
commitment. Back it up offline (hardware wallet, encrypted vault) at commit time.
→ [Security](security.md)

### Why was my commitment rejected as a duplicate?
Each commitment hash can only be registered once. Re-committing identical content
with the same blinding factor produces the same hash. Use a fresh random blinding
factor per commitment.

### How many IPs can I commit in one batch?
Up to `MAX_BATCH_SIZE = 50` via the API / `batch_commit_ip`. Larger batches risk
exceeding Soroban per-transaction resource limits.
→ [Soroban Resource Limits](soroban-resource-limits.md)

---

## Atomic Swaps

### What is the lifecycle of a swap?
`Pending` → `Accepted` → `Completed`, with side exits to `Cancelled`, `Disputed`
or `RolledBack`.
→ [Atomic Swap](atomic-swap.md)

### Why can't I initiate a second swap on the same IP?
Only one active swap per IP is allowed (`DataKey::ActiveSwap(ip_id)`). Cancel or
complete the existing one first.

### The buyer accepted but the seller never revealed the key. What now?
The buyer can cancel after expiry (`cancel_expired_swap`) to get funds back, or
raise a dispute within the dispute window.
→ [Dispute Resolution Guide](dispute-resolution-guide.md)

### What fees are charged?
The default protocol fee is 250 bps (2.5%) of the price, sent to the treasury on
completion. Referral fee defaults to 100 bps.

---

## Disputes

### How long do I have to raise a dispute?
By default 24 hours (`dispute_window_seconds = 86400`) after the swap is accepted.
After that `raise_dispute` fails with `DisputeExpired`.

### Why does submitting evidence cost tokens?
Each party's first evidence submission charges a dispute bond — the greater of
`MIN_DISPUTE_BOND` or 10% of the swap price — to discourage frivolous disputes. The
winning side's bond is refunded; the losing side's is forfeited.
→ [Dispute Resolution Guide](dispute-resolution-guide.md)

### What happens if nobody resolves my dispute?
After `dispute_timeout_secs` (default 7 days) anyone can call
`auto_resolve_dispute`, which refunds the buyer.

---

## API Server

### Which API version should I use?
`/v1/` (API version `1.0.0`). Send `Accept-Version: 1.0.0` to pin explicitly.
→ [API Migration Guide](api-migration-guide.md), [SDK Versioning](sdk-versioning.md)

### I get `406 Not Acceptable`. Why?
Your `Accept-Version` header names a version not in `SUPPORTED_VERSIONS`. Remove the
header or use a supported version (currently `1.0.0`, `1.1.0`).

### I get `401`/`403` on write endpoints.
Write endpoints (`/ip/commit`, `/ip/transfer`, `/swap/*` mutations) require request
signing. Check that your signature is valid and your clock skew is within
`REQUEST_SIGNATURE_SKEW_SECS`.
→ [Integration Guide](integration-guide.md)

### I get `429 Too Many Requests`.
You hit a rate-limit tier (Free / Premium / Enterprise). Honour the `Retry-After`
header and back off exponentially.

### Why are reads stale for a few seconds after a write?
The API caches IP (60 s) and swap (30 s) queries. Writes invalidate the relevant keys;
if Redis is unavailable the cache falls back to per-instance memory, so other
replicas may serve stale data until TTL expiry.
→ [Performance Tuning Guide](performance-tuning-guide.md#caching-strategies)

### Where is the error format documented?
→ [Error Response Schema](error-response-schema.md)

---

## Build, Deploy & Operations

### How do I build and deploy?
```bash
./scripts/build.sh
./scripts/deploy_testnet.sh
```
→ [README](../README.md), [Integration Guide](integration-guide.md)

### Which environment variables does the API server read?
`SOROBAN_RPC_URL`, `IP_REGISTRY_CONTRACT`, `REDIS_URL`, `DATABASE_URL`,
`REQUEST_SIGNATURE_SKEW_SECS`, `AUDIT_LOG_PATH`, `AUDIT_HMAC_KEY`, `OTEL_ENABLED`,
`OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_SERVICE_NAME`.

### My WASM is too large to deploy.
Check with `./scripts/check-wasm-size.sh` against `scripts/wasm-size-baseline.json`
and build in release mode with `opt-level = "z"`.
→ [Performance Tuning Guide](performance-tuning-guide.md#gas--resource-optimization)

### How do I report a security issue?
Privately, per [SECURITY.md](../SECURITY.md). Do not open a public issue.

---

## Troubleshooting Error Codes

| Code | Name | Common cause | Fix |
|------|------|--------------|-----|
| 17 | `DisputeWindowExpired` | Dispute raised after the window | Raise within 24 h of acceptance |
| 18 | `OnlyBuyerCanDispute` | Seller tried to raise a dispute | Only the buyer can call `raise_dispute` |
| 19 | `SwapNotDisputed` | Resolution call on a non-disputed swap | Check swap status first |
| 37 | `NoArbitratorSet` | Ruling before committee is set | Admin calls `set_arbitrator` |
| 38 | `UnauthorizedEvidenceSubmitter` | Third party submitted evidence | Only buyer/seller may submit |
| 58 | `CommitteeSizeTooSmall` | Committee < 3 signers or threshold < 2 | Use at least 2-of-3 |
| 59 | `EvidenceRequired` | Ruling with no evidence on file | Submit evidence first |
| 63 | `TimelockNotElapsed` | `execute_ruling` called too early | Wait for the ruling delay (48 h) |
| 64 | `RulingFinalized` | Cancel after time-lock expired | Ruling can no longer be cancelled |

Full list: `contracts/atomic_swap/src/errors.rs`, `contracts/ip_registry/src/errors.rs`.

---

## Maintaining this FAQ

### Update process
1. **Monthly review** — a docs maintainer checks each answer against the current
   code and bumps *Last reviewed* at the top.
2. **On release** — any PR that changes a documented default (fees, windows,
   timeouts, versions) must update the matching FAQ answer in the same PR.
3. **Link check** — relative links must resolve to files in `docs/`.

### Tracking unanswered questions
Questions that are asked but not yet answered here are tracked so they can be added:

1. Open a GitHub issue with the label `faq` and the title `FAQ: <question>`.
2. Include where the question came from (issue, PR, chat) and how often it appears.
3. During the monthly review, maintainers triage `faq` issues: answer and add to this
   file, or close with a link to existing docs.

| Question | Source | Status |
|----------|--------|--------|
| _Add pending questions here or track via the `faq` label_ | | |

### Writing guidelines
- One question per heading, phrased as the user would ask it.
- Answer in ≤ 5 sentences; link out for detail.
- Prefer concrete values (defaults, error codes) over vague descriptions.
