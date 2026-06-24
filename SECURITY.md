# Security Policy

## Overview

The AtomicIP project handles real XLM and intellectual property assets through Soroban smart contracts. Security is critical to protect users' funds and IP rights.

## Reporting a Vulnerability

We take security vulnerabilities seriously. If you discover a security vulnerability, please follow responsible disclosure practices.

### How to Report

**DO NOT** open a public GitHub issue for security vulnerabilities.

Instead, please report vulnerabilities via one of the following methods:

1. **Email**: Send a detailed report to security@atomicip.io
2. **GitHub Security Advisories**: Use the [Security Advisories](https://github.com/AtomicIP/AtomicIP-/security/advisories/new) page

### What to Include

When reporting a vulnerability, please include:

- Description of the vulnerability
- Steps to reproduce the issue
- Potential impact assessment
- Suggested fix (if available)
- Any relevant logs or screenshots

### Response Timeline

- **Initial Response**: Within 48 hours of receipt
- **Status Update**: Within 7 days
- **Fix Timeline**: Depends on severity, typically 14-30 days

### Disclosure Process

1. **Acknowledgment**: We will acknowledge receipt of your report within 48 hours
2. **Investigation**: Our team will investigate and validate the vulnerability
3. **Fix Development**: We will develop and test a fix
4. **Disclosure**: We will coordinate disclosure with you after the fix is deployed
5. **Credit**: We will credit you in the security advisory (unless you prefer anonymity)

## Security Best Practices for Users

### For IP Owners

- **Keep your secret safe**: The secret used to create your commitment hash is the only way to prove ownership. Store it securely offline.
- **Verify commitment hashes**: Before committing, verify your commitment hash is correctly computed: `sha256(secret || blinding_factor)`
- **Use strong secrets**: Use cryptographically secure random values for secrets and blinding factors
- **Backup your keys**: Maintain secure backups of your Stellar wallet keys

### For Swap Participants

- **Verify swap details**: Always verify the IP ID, price, and counterparty before accepting a swap
- **Check expiry times**: Be aware of swap expiry times to avoid losing funds
- **Use trusted registries**: Only interact with verified IP registry contracts
- **Monitor transactions**: Review transaction details before signing

## Known Limitations

### Current Limitations

1. **No Token Escrow**: The current implementation does not escrow tokens during swaps. Payment is transferred to the contract but not held in escrow. This will be addressed in v1.1.

2. **Single Network**: Currently only supports Stellar testnet. Mainnet support is planned for v1.0.

3. **No Partial Disclosure**: The commitment scheme requires full secret revelation. Partial disclosure proofs are planned for v2.0.

4. **Gas Costs**: Complex operations may have higher gas costs. Optimization is ongoing.

5. **Frontend Not Included**: The current repository contains only smart contracts. A frontend UI is planned for v3.0.

### Security Assumptions

- Users maintain secure storage of their secrets and private keys
- The Stellar network operates as expected
- Soroban runtime is secure and bug-free
- Cryptographic primitives (SHA256) are secure

## Security Features

### Implemented

- ✅ Pedersen commitment scheme for IP privacy
- ✅ Atomic swap with key verification
- ✅ Authorization checks via `require_auth()`
- ✅ Duplicate commitment prevention
- ✅ Expiry-based cancellation for buyers
- ✅ Monotonic ID generation (upgrade-safe)

### Planned

- 🔄 Token escrow in atomic swaps
- 🔄 Multi-signature support
- 🔄 Time-locked commitments
- 🔄 Partial disclosure proofs

## Automated Security Scanning

Every push and pull request is scanned automatically in CI/CD:

- **Security scanning** — supply-chain policy (cargo-deny), secret detection
  (gitleaks), and static analysis. See [Security Scanning](docs/security-scanning.md).
- **Dependency vulnerability scanning** — cargo-audit + cargo-deny against the
  RustSec advisory database, plus Dependabot. See [Dependency Scanning](docs/dependency-scanning.md).
- **Code coverage enforcement** — a minimum coverage threshold is enforced in
  CI. See [Code Coverage](docs/code-coverage.md).
- **Mutation testing** — verifies the test suite catches logic errors. See
  [Mutation Testing](docs/mutation-testing.md).

Run all gates locally with `./scripts/security-checks.sh`.

## Security Audits

### Audit Status

- **Initial Review**: Internal security review completed
- **External Audit**: Planned for Q2 2026
- **Bug Bounty**: Planned for post-mainnet launch

### Audit Reports

Audit reports will be published in the [security-advisories](https://github.com/AtomicIP/AtomicIP-/security/advisories) section after completion.

## Contact

For security-related inquiries:

- **Security Team**: security@atomicip.io
- **General Contact**: contact@atomicip.io
- **GitHub**: [Security Advisories](https://github.com/AtomicIP/AtomicIP-/security/advisories)

## Bug Bounty Program (Planned)

We plan to launch a bug bounty program after mainnet launch. Rewards will be based on severity:

- **Critical**: $5,000 - $25,000
- **High**: $1,000 - $5,000
- **Medium**: $500 - $1,000
- **Low**: $100 - $500

Details will be published at [bugbounty.atomicip.io](https://bugbounty.atomicip.io) when the program launches.

## Compliance and Data Protection (#634)

### GDPR Compliance

AtomicIP is designed to comply with the General Data Protection Regulation (GDPR) requirements for user data protection.

#### Data Controller and Processor

- **Data Controller**: AtomicIP Foundation (contact@atomicip.io)
- **Data Processor**: Stellar Network (for on-chain data)
- **Data Protection Officer**: dpo@atomicip.io

#### Data Collected

| Data Type | Purpose | Retention Period | Storage Location |
|-----------|---------|-----------------|-----------------|
| Stellar Public Address | IP ownership, swap identification | 90 days (off-chain cache) | In-memory cache + Stellar ledger |
| Commitment Hash | IP timestamping proof | Indefinite (on-chain) | Stellar ledger |
| Swap Records | Patent sale execution | 365 days (off-chain cache) | In-memory cache + Stellar ledger |
| Audit Logs | Security monitoring | 365 days | In-memory audit store |
| IP Address (HTTP) | Rate limiting, abuse prevention | Session only | Runtime memory |
| Request Signatures | Request authentication | Session only | Runtime memory |

#### GDPR Rights Implementation

| GDPR Article | Right | Implementation | Endpoint |
|---|---|---|---|
| Art. 15 | Right of Access | Data export endpoint returns all user data | `POST /v1/gdpr/export` |
| Art. 16 | Right to Rectification | User can update commitment metadata | Via Stellar transaction |
| Art. 17 | Right to Erasure | Data deletion cascade for user records | `POST /v1/gdpr/delete` |
| Art. 18 | Right to Restrict Processing | Opt-out of non-essential processing | Contact DPO |
| Art. 20 | Right to Data Portability | Machine-readable JSON export | `POST /v1/gdpr/export` |
| Art. 21 | Right to Object | Object to data processing | Contact DPO |
| Art. 22 | Automated Decision-Making | Atomic swaps are user-initiated | N/A |

#### Data Retention Policy

| Data Category | Retention Period | Rationale |
|---|---|---|
| IP Records (on-chain) | Indefinite (ledger) | Immutable blockchain requirement |
| IP Records (cache) | 60 seconds | Cache TTL for performance |
| Swap Records (on-chain) | Indefinite (ledger) | Immutable blockchain requirement |
| Swap Records (cache) | 30 seconds | Cache TTL for performance |
| Audit Logs | 365 days | Security monitoring compliance |
| Webhook Event Records | 7 days | Delivery tracking and debugging |
| Rate Limiter State | 15 minutes | Abuse prevention |
| Idempotency Keys | 1 hour | Duplicate request prevention |

#### Data Deletion Cascade

When a user requests data deletion (`POST /v1/gdpr/delete`), the following actions occur:

1. **Cache Invalidation**: All cached IP lists, swap lists, and reputation data for the user are immediately invalidated
2. **On-Chain Data**: Smart contract data (IP records, swaps) cannot be deleted from the immutable ledger, but IP records can be revoked
3. **Audit Log**: Audit events linked to the user are anonymized
4. **Webhook Events**: Pending webhook deliveries for the user are cancelled
5. **Confirmation Required**: The request must include a `confirmation: "DELETE"` field to prevent accidental deletions

#### Data Export Format

Data export responses (`POST /v1/gdpr/export`) include:
- User's Stellar address
- All IP records owned by the user
- All swap records involving the user
- All audit events linked to the user
- Export timestamp and data retention period

### Accessibility Compliance

API responses are designed to be accessible to all clients:

- **JSON Schema Consistency**: All responses use snake_case field names, consistent pagination structure, and machine-readable data types
- **Error Format Consistency**: All errors return `{"error": "message"}` JSON format
- **Content-Type**: All responses use `application/json` content type
- **Version Negotiation**: Clients can specify API version via `Accept-Version` or `X-API-Version` headers
- **Public Endpoints**: Health, docs, and version endpoints require no authentication
- **Minimal Payloads**: All mutation endpoints accept minimal payloads with only required fields
- **Machine-Readable Timestamps**: All timestamps use Unix epoch (u64 seconds)
- **Nullable Fields**: Nullable fields use explicit `null` instead of field absence

### WCAG Compliance for API

While WCAG (Web Content Accessibility Guidelines) primarily applies to user interfaces, our API follows accessibility best practices:

1. **Perceivable**: JSON responses are machine-readable and parseable by any HTTP client
2. **Operable**: All endpoints are navigable via URI patterns; pagination prevents timeout
3. **Understandable**: Consistent error formats, snake_case naming, and semantic versioning
4. **Robust**: Responses include all required fields even when empty; content negotiation via Accept header

### Data Protection Impact Assessment (DPIA)

A Data Protection Impact Assessment has been conducted for the following processing activities:

- IP commitment and ownership tracking (on-chain)
- Atomic swap execution (on-chain)
- API request logging and audit trails
- Webhook event delivery

**Risk Level**: Medium — The system primarily processes cryptographic hashes and Stellar addresses, not personal identifiable information (PII). The immutable nature of blockchain data is mitigated by the use of commitment hashes rather than raw content.

### Breach Notification Procedure

In the event of a data breach:

1. Internal detection and verification (within 24 hours)
2. Containment measures applied
3. Supervisory authority notification (within 72 hours per GDPR Art. 33)
4. Affected users notified (without undue delay per GDPR Art. 34)
5. Post-incident review and remediation

### Compliance Testing

Automated compliance tests run in CI/CD to verify:

- Error response format compliance
- Health endpoint required fields
- API versioning enforcement
- GDPR data export and deletion request validation
- Data retention policy declaration
- Cache invalidation on data deletion
- JSON schema consistency
- Webhook delivery status tracking

## Legal

This security policy is subject to our [Terms of Service](https://atomicip.io/terms) and [Privacy Policy](https://atomicip.io/privacy).

---

**Last Updated**: 2026-06-24
**Version**: 2.0.0
