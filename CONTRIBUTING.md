# Contributing to Atomic Patent

Thank you for your interest in contributing to Atomic Patent! This document outlines our contribution guidelines and development workflow.

## Code of Conduct

We are committed to providing a welcoming and inclusive environment for all contributors.

## Getting Started

### Prerequisites

- Rust 1.70+
- Node.js 16+ and npm
- Soroban CLI
- Stellar CLI
- Redis (for local testing)

### Development Setup

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR_USERNAME/AtomicIP-.git`
3. Add upstream: `git remote add upstream https://github.com/AtomicIP/AtomicIP-.git`
4. Create a feature branch: `git checkout -b feat/your-feature`

### Building and Testing

```bash
# Install dependencies
npm install
cargo build --workspace

# Run all tests
./scripts/test.sh        # Rust tests
npm test                 # JS tests with coverage

# Run specific test suites
cargo test --package ip_registry
npm test -- path/to/test.js

# Check formatting
npm run format:check
cargo fmt --check

# Lint code
npm run lint
cargo clippy --workspace
```

## Merge Conflict FIXME Policy

**Critical:** PRs must not be merged with commented-out tests or disabled modules due to unresolved merge conflicts, even under deadline pressure.

### What This Means

When a merge conflict occurs during development:

1. **Do Not** leave disabled test modules (e.g., `// mod tests;`) in the PR
2. **Do Not** commit temporary `// FIXME #XXX: merge conflict` comments without:
   - Opening a corresponding tracking issue
   - Removing the disabled module/test
   - Or resolving the merge conflict itself

3. **Prefer** resolving the conflict immediately by:
   - Rebasing onto the latest main
   - Manually reconciling changes
   - Running full test suite to verify resolution

### CI Enforcement

Our CI pipeline includes automated checks:
- Gitleaks scans for secrets
- Merge-conflict FIXME detector validates no stale disabled modules
- All Rust and JavaScript tests must pass
- Format and lint checks enforced

**Blocked modules** (tracked separately, allowed to remain commented):
- `contracts/ip_registry/src/benchmarks.rs` — Issue #817
- `contracts/ip_registry/src/invariant_tests.rs` — Tracked in separate issue

Any **new** merge-conflict FIXMEs outside the allowlist will block merge.

## Pull Request Workflow

### Before Submitting

1. **Update from main**: `git rebase upstream/main`
2. **Run full test suite**:
   ```bash
   ./scripts/test.sh && npm test && cargo fmt --check && npm run format:check
   ```
3. **Check for disabled tests**: Run CI checks locally if possible
4. **One feature per branch**: Keep PRs focused and reviewable

### Creating a PR

1. Push your branch: `git push origin feat/your-feature`
2. Open a PR against `main`
3. Link any related issues: "Fixes #XXX" or "Related to #XXX"
4. Provide a clear description of changes and rationale
5. Ensure CI checks pass

### Review Process

- At least one approval required before merge
- All CI checks must pass
- Address review feedback with new commits (no force-pushing during review)
- Use "Squash and merge" for single-commit clarity when appropriate

## Branch Protection Rules

The following checks are **required to pass** before merging to `main`:

| Check | Type | Purpose |
|-------|------|---------|
| Gitleaks Secret Scan | Automated | Prevent credential leaks |
| Disabled Test Check | Automated | Enforce merge-conflict FIXME policy |
| Rust Build | Automated | Verify compilation |
| Rust Tests | Automated | Functional correctness |
| Rust Clippy Lint | Automated | Code quality |
| JavaScript Lint | Automated | Code standards |
| JavaScript Format | Automated | Consistent styling |
| JavaScript Tests | Automated | Functional correctness |

**At least 1 approval** is required from the team.

## Areas of Contribution

### Smart Contracts (Rust)

- `/contracts` — Soroban contracts
- Test coverage for new features
- Performance optimizations
- Security audits and fixes

### API Server (Rust)

- `/api-server` — HTTP API and cache layer
- Load balancer improvements
- Integration testing
- Redis interaction fixes

### JavaScript SDK & Tests

- `/src` — SDK and utilities (JSDoc-annotated)
- `/tests` — Integration and unit tests
- Type safety (expanding JSDoc coverage)
- Documentation and examples

### Documentation

- `/docs` — Architecture, design docs
- README updates
- API reference improvements
- Deployment guides

## Commit Messages

Write clear, descriptive commit messages:

```
feat: add new feature description

More detailed explanation of the changes, why they were made,
and any relevant context. Reference issues when applicable.

Fixes #123
```

**Format:**
- Start with conventional type: `feat`, `fix`, `docs`, `test`, `refactor`, `chore`
- Keep subject line under 50 characters
- Separate subject from body with blank line
- Wrap body at 72 characters
- Reference issues: `Fixes #XXX`, `Related to #XXX`

## Running Tests Locally

### Rust Tests

```bash
# Full test suite
cargo test --workspace --verbose

# Specific package
cargo test --package ip_registry

# With output
cargo test -- --nocapture
```

### JavaScript Tests

```bash
# Run all tests with coverage
npm test

# Watch mode
npm run test:watch

# Specific test file
npm test -- src/__tests__/atomic-swap.test.js
```

### Integration Tests

```bash
# Start Redis (if not running)
redis-server

# Run integration tests
REDIS_URL=redis://localhost:6379 cargo test --test '*'
```

## Reporting Issues

When reporting a bug:

1. Check if the issue already exists
2. Include reproduction steps
3. Provide environment details (OS, Rust version, Node version)
4. Share error messages and logs

For security issues, see [SECURITY.md](SECURITY.md).

## Documentation Style

- Use Markdown for all docs
- Code examples should be tested
- Reference related documentation
- Update table of contents if adding sections
- Keep API documentation in-code (Rust doc comments, JSDoc)

## CI/CD and Deployment

- CI runs on every push and PR
- Deployments to testnet trigger on release tags (`v*`)
- All tests and checks must pass before merging
- Never bypass or force-push CI checks

## Questions?

- Check [docs/](docs/) for architecture and design
- Review [SECURITY.md](SECURITY.md) for security guidelines
- Open an issue for discussion
- Reach out to maintainers

Thank you for contributing to Atomic Patent! 🚀
