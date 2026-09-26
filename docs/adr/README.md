# Architecture Decision Records

> Issue #1063

An ADR records one significant design decision: the context it was made in, the
options considered, what was chosen and the consequences. ADRs keep the
*why* behind the code after the people who made the decision have moved on.

## Index

| # | Title | Status |
|---|-------|--------|
| [0000](0000-template.md) | ADR template | — |
| [0001](0001-record-architecture-decisions.md) | Record architecture decisions | Accepted |
| [0002](0002-pedersen-commitments-for-privacy.md) | Pedersen commitments for IP privacy | Accepted |
| [0003](0003-stellar-soroban-for-settlement.md) | Stellar / Soroban for settlement | Accepted |
| [0004](0004-alert-fatigue-reduction-pipeline.md) | Alert fatigue reduction pipeline | Accepted |

## Storage and naming

- ADRs live in `docs/adr/` in this repository, versioned with the code they
  describe.
- File name: `NNNN-kebab-case-title.md`, where `NNNN` is the next free
  zero-padded number. Numbers are never reused.
- Add every new ADR to the index above in the same PR.

## Lifecycle

```
Proposed ──review──▶ Accepted ──(later ADR)──▶ Superseded by NNNN
    │                    │
    └──▶ Rejected        └──▶ Deprecated
```

Once an ADR is **Accepted**, its body is immutable. Only its status line may
change. To change a decision, write a new ADR that supersedes it, and update the
old one's status to `Superseded by [NNNN](NNNN-…md)`.

## When to write an ADR

Write one when a change:

- is hard or expensive to reverse (cryptographic scheme, storage layout,
  chain/platform, public API shape, upgrade mechanism);
- affects security or privacy guarantees;
- introduces a new dependency, service or protocol;
- chooses between several reasonable options that reviewers are likely to ask about.

If in doubt, write a short one. Most ADRs should fit on one screen.

## Review process

1. Copy [`0000-template.md`](0000-template.md) to the next number and set
   **Status: Proposed**.
2. Open a PR titled `ADR-NNNN: <title>` with the `adr` label. The ADR can go in
   the same PR as the implementation or before it.
3. Request review from at least **two maintainers**. For decisions that touch
   cryptography or contract storage, one of them must be a security reviewer
   (see `SECURITY.md`).
4. Keep discussion in the PR. Update the *Options considered* section with any
   alternative raised in review, including why it was not chosen.
5. The review period is at least **3 business days**, unless the ADR only
   records a decision that has already shipped.
6. On approval, set **Status: Accepted**, fill in the date, update the index
   and merge. If rejected, merge with **Status: Rejected** so the reasoning is
   kept.
7. ADRs are revisited in the quarterly architecture review. Any ADR whose
   *Consequences* no longer hold gets a superseding ADR.

## Linking ADRs from code

Reference the ADR in the module-level doc comment of the code that implements
the decision, using the repo-relative path:

```rust
//! Design rationale: docs/adr/0002-pedersen-commitments-for-privacy.md
```

Current links:

| ADR | Referenced from |
|-----|-----------------|
| 0002 | `contracts/ip_registry/src/zk_commitment.rs`, `contracts/ip_registry/src/lib.rs` |
| 0003 | `contracts/atomic_swap/src/lib.rs`, `contracts/ip_registry/src/lib.rs` |
| 0004 | `api-server/src/alerting.rs`, `api-server/src/commitment_monitoring.rs` |

To find stale links, run `grep -rn "docs/adr/" contracts api-server src`.
