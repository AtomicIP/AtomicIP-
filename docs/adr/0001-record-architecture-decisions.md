# ADR-0001: Record architecture decisions

- **Status:** Accepted
- **Date:** 2026-09-26
- **Related issues / PRs:** #1063

## Context

AtomicIP combines on-chain contracts, cryptographic commitment schemes and an
off-chain API server. The reasons behind core choices, such as the commitment
scheme or the settlement chain, were only written in PR threads and chat. New
contributors and auditors kept asking the same questions, and some changes
were proposed that would have silently broken earlier security assumptions.

## Decision

We will record significant architecture decisions as ADRs in `docs/adr/` using
the template and review process described in [README.md](README.md).

## Consequences

- Design rationale is versioned next to the code and survives team changes.
- Adds a small amount of process to significant changes.
- Existing key decisions are backfilled (ADR-0002, ADR-0003).
