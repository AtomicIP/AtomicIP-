# Security Scanning and Test Quality

This guide distinguishes checks run by GitHub Actions from additional Rust checks
that contributors can run locally. The CI configuration is in
[`.github/workflows/ci.yml`](../.github/workflows/ci.yml).

## Security Scanning

CI runs [Gitleaks](../.github/workflows/ci.yml) on pushes and pull requests to detect
secrets. The repository's Gitleaks rules are in [`.gitleaks.toml`](../.gitleaks.toml).
Run the supply-chain policy check locally with:

```bash
./scripts/security-checks.sh deny
```

## Dependency Scanning

The local security-check script provides both Rust dependency checks:

```bash
./scripts/security-checks.sh audit  # RustSec advisories via cargo-audit
./scripts/security-checks.sh deny   # advisories, bans, licenses, and sources
```

These checks are not currently run by the GitHub Actions workflow.
Dependabot checks Cargo and npm dependencies weekly; its configuration is in
[`.github/dependabot.yml`](../.github/dependabot.yml).

## Code Coverage

CI runs JavaScript tests with coverage and enforces configured thresholds with
[`scripts/coverage-threshold-gate.sh`](../scripts/coverage-threshold-gate.sh).
The local Rust coverage gate uses `cargo-tarpaulin` and defaults to 70%:

```bash
./scripts/security-checks.sh coverage
```

## Mutation Testing

The local mutation gate uses `cargo-mutants` for the `ip_registry` and `atomic_swap`
packages:

```bash
./scripts/security-checks.sh mutants
```

For contract-specific strategy and examples, see the
[Atomic Swap mutation testing guide](../contracts/atomic_swap/MUTATION_TESTING.md).

## Run All Local Rust Gates

```bash
./scripts/security-checks.sh
```