# Changelog

All notable changes to Atomic Patent are documented in this file. This changelog tracks issue numbers referenced throughout the codebase, organized chronologically to help reconstruct the evolution of features and fixes.

## How to Use This File

Each entry below references an issue number from our GitHub repository. When reviewing code comments mentioning an issue, refer to this file to understand the context, rationale, and chronological ordering of changes.

## Issue Tracking by Category

### Core Swap Functionality
- **#35** — Refund buyer's escrowed payment on swap cancellation
- **#251** — Buyer cancel pending swap on timeout
- **#252** — Seller extend swap expiry
- **#253** — Swap history / audit trail with logging of all swap state transitions
- **#254** — Multi-sig approval workflow for atomic swaps

### Referral & Fee System
- **#311** — Referral reward tracking and fee deduction from seller proceeds
- **#309** — Batch swap initiation support

### Arbitration & Dispute Resolution
- **#313** — Dispute evidence submission and validation
- **#314** — Arbitration mechanism and arbitrator assignment
- **#355** — Arbitrator address assignment for dispute resolution
- **#356** — Atomic refund processing for disputed swaps
- **#357** — Escalation mechanisms for unresolved disputes
- **#358** — Timeout extension and expiry escalation
- **#359** — Committee-based arbitration (initial draft)
- **#360** — Evidence requirements and validation rules

### IP Auction Mechanism
- **#347** — IP auction mechanism with bid tracking and price discovery

### Payment & Escrow Features
- **#349** — Scheduled payment support for installment-based sales
- **#350** — Collateral escrow management
- **#351** — Escrow agent assignment and role-based release
- **#352** — Renegotiation offer support for extended swaps
- **#353** — Insurance premium and claims handling
- **#354** — Insurance pool management and reserve validation

### Oracle Integration
- **#466** — Price oracle configuration and setup
- **#468** — Oracle price validation bounds
- **#470** — Oracle integration for automated swap pricing
- **#784** — Oracle price deviation checking (max deviation threshold)

### Batch Operations & Idempotency
- **#515** — Batch fingerprint tracking for idempotent results
- **#516** — Batch processing coordination
- **#517** — Batch error handling and rollback
- **#518** — Batch validation rules
- **#519** — Batch completion tracking
- **#520** — Batch history and audit trail
- **#521** — Batch cost optimization
- **#522** — Batch operation concurrency
- **#523** — Idempotent batch fingerprint mapping

### Reputation & Compliance
- **#824** — Reputation scoring system
- **#825** — Reputation multiplier per IP ID
- **#828** — Reputation validation in swap acceptance
- **#829** — Reputation persistence and updates
- **#830** — Reputation decay and time-based adjustments
- **#831** — Reputation threshold enforcement
- **#832** — Reputation transfer and delegation

### Security & Hardening
- **#66-67** — Error code definitions and security validations
- **#781** — Arbitrator committee mechanism with M-of-N signatures and time-locked ruling enforcement
- **#906** — Treasury address validation: guard against hardcoded placeholder addresses

## Contributing

When adding new features or fixes, update this file with:
1. Issue number (from GitHub)
2. Concise description of the change
3. Any cross-references to related issues
4. Placement in the appropriate category section

Every merged PR that touches contract logic or introduces new features should include a CHANGELOG entry.

## Version Release Schedule

Release tags follow semantic versioning (`v1.0.0`, `v1.1.0`, etc.) and are created based on feature readiness, not calendar-based schedules. See the [GitHub Releases](https://github.com/AtomicIP/AtomicIP-/releases) page for version history.
