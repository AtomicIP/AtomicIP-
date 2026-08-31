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

## API Server Audit Log

The API server (`api-server/src/audit.rs`) maintains a hash-chained
(HMAC-SHA256) audit log of commitment, swap, and admin events.

- **Durability**: every event is appended to a durable, append-only file and
  fsynced before the write is acknowledged. On startup, the store replays
  that file to resume the existing chain (`previous_signature` and
  `sequence`) instead of starting a new one — a process restart does not
  reset or fork the chain.
- **Key management**: `AUDIT_HMAC_KEY` must be set from durable
  configuration/secret storage. If it is absent, the server fails to start
  rather than silently signing the chain with a randomly generated,
  unrecoverable key.
- **Multi-instance safety**: concurrent instances sharing the same backing
  store coordinate appends through an exclusive file lock and always derive
  the next sequence number from the store's own current tail, so replicas
  behind a load balancer cannot assign colliding sequence numbers.
- **Independent verification**: `audit::verify_persisted_chain` walks the
  persisted file and confirms every signature matches its
  `previous_signature` linkage. It only needs read access to the backing
  file and the HMAC key, so an outside auditor can run it without any
  cooperation from a live server instance.

## Test Re-enablement Security Audit Checklist

When re-enabling a previously-disabled test module (especially those related to fund safety, upgrade safety, or security-critical operations), the following security review must be performed:

### Mandatory Review Items

- [ ] **Security impact assessment**: Document why the test was originally disabled and verify the underlying issue is resolved
- [ ] **Authorization checks**: Verify all entry points in the test's target code call `require_auth()` appropriately
- [ ] **Fund safety**: If the test covers fund operations (escrow, transfers, swaps), ensure no edge cases allow fund loss or theft
- [ ] **State invariants**: Confirm that the test validates critical state invariants (e.g., token conservation, ownership verification)
- [ ] **Upgrade safety**: For any disabled tests related to contract upgrades, verify backwards compatibility and migration safety
- [ ] **Threat model coverage**: Ensure the re-enabled test covers attack scenarios documented in `docs/threat-model.md`
- [ ] **Code review**: Obtain at least one approval from a team member familiar with the affected module
- [ ] **Manual testing**: Perform manual testing against testnet if the test covers user-facing functionality

### Test Modules Requiring Security Sign-Off

The following disabled test modules are security-critical and require the checklist above before re-enablement:

- `upgrade_chaos_tests` (#19) — covers contract upgrade safety and state consistency
- `escrow_tests` (#20) — covers fund safety and payment verification
- `batch_swap_features_tests` (#22) — covers atomic swap correctness and fund recovery

### Cross-Reference

For complete threat analysis, refer to:
- `docs/threat-model.md` for attack scenarios
- `SECURITY.md` for API server audit log procedures
- The PR that re-enables the test must include security sign-off notes

## Secret Scanning & Incident Response

### Automated Secret Detection

Every commit is scanned automatically in CI/CD using **Gitleaks** to detect:

- API keys, JWT secrets, and authentication tokens
- Stellar keypairs and private keys
- Database connection strings with embedded credentials
- HMAC keys and other cryptographic material

Configuration is defined in `.gitleaks.toml` with patterns for:
- Stellar keypairs (S-prefixed 56-character strings)
- JWT secrets and bearer tokens
- Generic API keys (api_key, apikey patterns)
- Database passwords in connection strings
- HMAC signing keys

**Scan Coverage**:
- **Full history**: On initial CI setup or when requested, the entire git history is scanned
- **PR diffs**: Every pull request scans only the changed lines against baseline
- **Allowlist**: Common false positives (example_, test_, placeholder) are excluded

### Incident Response: If a Secret is Found

If a secret is detected in the commit history (either pre-emptively in CI or discovered post-facto):

1. **Immediate Actions**:
   - If the secret is a **Stellar keypair or API credential**: rotate the key immediately on the affected service (Stellar account, API server, database)
   - If the secret is a **test/example key**: verify it is only for testing; verify the file is in `.gitignore` and will not be committed
   - Open a **private security issue** (not public) to coordinate response

2. **Remediation**:
   - If the commit is on `main` or already published: use `git filter-repo` or `BFG Repo-Cleaner` to remove the secret from history
   - Re-write the git history to remove the commit containing the secret
   - Force-push the cleaned history (with security team approval)
   - Notify users and stakeholders of the exposure window

3. **Audit & Documentation**:
   - Log the incident in the **security advisory** section: who discovered it, when, what was exposed, and the remediation steps
   - Update threat model or security posture documentation if the incident reveals a new risk
   - Review `.gitleaks.toml` configuration to ensure the secret pattern is detected by the scanner

4. **Prevention**:
   - Add the secret pattern to `.gitleaks.toml` if it is not already covered
   - Ensure all developers run Gitleaks locally before committing (`gitleaks detect --source . -v`)
   - Add a pre-commit hook to reject commits with detected secrets (see [Pre-Commit Hooks](#pre-commit-hooks))

### Running Gitleaks Locally

Before committing, scan your changes:

```bash
gitleaks detect --source . -v
```

To scan the full repository history:

```bash
gitleaks detect --source . --verbose
```

To ignore a specific secret (use sparingly):

```bash
echo "allowlist: [<SECRET_LINE>]" >> .gitleaks.toml
```

### Pre-Commit Hooks

Add this to `.git/hooks/pre-commit` to block commits with secrets:

```bash
#!/bin/bash
gitleaks detect --staged -v --exit-code 1
```

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

## Legal

This security policy is subject to our [Terms of Service](https://atomicip.io/terms) and [Privacy Policy](https://atomicip.io/privacy).

---

**Last Updated**: 2026-03-27
**Version**: 1.0.0
