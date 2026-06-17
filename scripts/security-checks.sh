#!/usr/bin/env bash
# security-checks.sh
# Run the CI security / quality gates locally, mirroring the GitHub workflows:
#   security.yml (#553), dependency-scan.yml (#554),
#   coverage.yml (#555), mutation.yml (#556).
#
# Usage:
#   ./scripts/security-checks.sh            # run all checks
#   ./scripts/security-checks.sh audit      # run a single check
#   COVERAGE_THRESHOLD=80 ./scripts/security-checks.sh coverage
#
# Checks: deny | audit | coverage | mutants | all (default)
set -euo pipefail
source "$HOME/.cargo/env" 2>/dev/null || true

COVERAGE_THRESHOLD="${COVERAGE_THRESHOLD:-70}"
CHECK="${1:-all}"

have() { command -v "$1" >/dev/null 2>&1; }

ensure() {
    # ensure <binary> <crate>
    if ! have "$1"; then
        echo ">> installing $2 ..."
        cargo install "$2" --locked
    fi
}

run_deny() {
    echo "== cargo-deny (advisories, bans, licenses, sources) =="
    ensure cargo-deny cargo-deny
    cargo deny check
}

run_audit() {
    echo "== cargo-audit (dependency vulnerabilities) =="
    ensure cargo-audit cargo-audit
    cargo audit
}

run_coverage() {
    echo "== cargo-tarpaulin (enforce >= ${COVERAGE_THRESHOLD}%) =="
    ensure cargo-tarpaulin cargo-tarpaulin
    cargo tarpaulin --workspace --out Xml --output-dir coverage \
        --fail-under "${COVERAGE_THRESHOLD}"
}

run_mutants() {
    echo "== cargo-mutants (mutation testing) =="
    ensure cargo-mutants cargo-mutants
    cargo mutants --no-shuffle -p ip_registry -p atomic_swap
}

case "$CHECK" in
    deny)     run_deny ;;
    audit)    run_audit ;;
    coverage) run_coverage ;;
    mutants)  run_mutants ;;
    all)
        run_deny
        run_audit
        run_coverage
        run_mutants
        ;;
    *)
        echo "Unknown check: $CHECK" >&2
        echo "Valid: deny | audit | coverage | mutants | all" >&2
        exit 2
        ;;
esac

echo "Security checks complete."
