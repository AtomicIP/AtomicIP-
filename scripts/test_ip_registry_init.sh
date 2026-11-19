#!/usr/bin/env bash

################################################################################
# Tests for #1259: IP Registry admin initialization in deploy.sh and
# scripts/verify_deployment.sh
#
# verify_deployment.sh is exercised against a stub `stellar` binary so no
# network access is needed. Each stub mode mimics a CLI response.
#
# Run with: ./scripts/test_ip_registry_init.sh
################################################################################

set -u

SCRIPTS_DIR="$(cd "$(dirname "$0")" && pwd)"
DEPLOY_SCRIPT="$SCRIPTS_DIR/deploy.sh"
VERIFY_SCRIPT="$SCRIPTS_DIR/verify_deployment.sh"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT
failures=0

# Write a stub `stellar` whose invoke behaviour is given by $1.
make_stub() {
    local mode="$1"
    mkdir -p "$WORK_DIR/bin"
    cat > "$WORK_DIR/bin/stellar" << STUB
#!/usr/bin/env bash
if [[ "\$1" == "keys" && "\$2" == "address" ]]; then
    echo "GDEPLOYERTESTADDRESS"
    exit 0
fi
case "$mode" in
    initialized)
        echo "error: transaction simulation failed: Error(Contract, #37)" >&2
        exit 1 ;;
    uninitialized)
        echo "ok"
        exit 0 ;;
    unrelated)
        echo "error: network timeout" >&2
        exit 1 ;;
esac
STUB
    chmod +x "$WORK_DIR/bin/stellar"
}

run_verify() {
    OUTPUT="$(PATH="$WORK_DIR/bin:$PATH" bash "$VERIFY_SCRIPT" CABC testnet 2>&1)"
    STATUS=$?
}

check() {
    local name="$1"; shift
    if "$@"; then
        echo "PASS  $name"
    else
        echo "FAIL  $name"
        failures=$((failures + 1))
    fi
}

test_verify_passes_when_admin_set() {
    make_stub initialized
    run_verify
    [[ $STATUS -eq 0 ]] && grep -q "admin is set" <<< "$OUTPUT"
}

test_verify_fails_when_admin_missing() {
    make_stub uninitialized
    run_verify
    [[ $STATUS -ne 0 ]] && grep -q "admin is not set" <<< "$OUTPUT"
}

test_verify_fails_on_unrelated_error() {
    make_stub unrelated
    run_verify
    [[ $STATUS -ne 0 ]] && grep -q "could not confirm" <<< "$OUTPUT"
}

test_verify_requires_arguments() {
    OUTPUT="$(bash "$VERIFY_SCRIPT" 2>&1)"
    STATUS=$?
    [[ $STATUS -ne 0 ]] && grep -q "usage:" <<< "$OUTPUT"
}

test_verify_simulates_only() {
    make_stub initialized
    run_verify
    grep -q -- "--send=no" "$VERIFY_SCRIPT"
}

test_ip_registry_initialized_before_swap() {
    local registry_line swap_line
    registry_line=$(grep -n 'initialize_ip_registry$' "$DEPLOY_SCRIPT" | head -1 | cut -d: -f1)
    swap_line=$(grep -n 'Initializing Atomic Swap with IP Registry address' "$DEPLOY_SCRIPT" | head -1 | cut -d: -f1)
    [[ -n "$registry_line" && -n "$swap_line" && "$registry_line" -lt "$swap_line" ]]
}

test_ip_registry_init_passes_admin_argument() {
    grep -q -- '--admin "\$admin_address"' "$DEPLOY_SCRIPT"
}

test_validate_calls_verify_script() {
    grep -q 'verify_deployment.sh' "$DEPLOY_SCRIPT"
}

check "verify passes when the registry admin is set" test_verify_passes_when_admin_set
check "verify fails when the registry admin is missing" test_verify_fails_when_admin_missing
check "verify fails on an unrelated error" test_verify_fails_on_unrelated_error
check "verify requires contract id and network" test_verify_requires_arguments
check "verify uses simulation only (--send=no)" test_verify_simulates_only
check "deploy initializes IP Registry before the swap" test_ip_registry_initialized_before_swap
check "deploy passes deployer as the registry admin" test_ip_registry_init_passes_admin_argument
check "validate step calls verify_deployment.sh" test_validate_calls_verify_script

if [[ $failures -gt 0 ]]; then
    echo "$failures check(s) failed"
    exit 1
fi
echo "All IP Registry init checks passed"
