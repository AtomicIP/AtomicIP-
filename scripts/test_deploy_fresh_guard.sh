#!/usr/bin/env bash

################################################################################
# Tests for #1258: mainnet guard for FRESH_DEPLOY in scripts/deploy.sh
#
# The guard runs before any tool check or key operation, so these tests need
# no Stellar CLI. Each case runs deploy.sh from a scratch directory so the
# log file it writes does not land in the repository.
#
# Run with: ./scripts/test_deploy_fresh_guard.sh
################################################################################

set -u

DEPLOY_SCRIPT="$(cd "$(dirname "$0")" && pwd)/deploy.sh"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT
failures=0

# run_deploy <args...>: sets OUTPUT and STATUS
run_deploy() {
    OUTPUT="$(cd "$WORK_DIR" && bash "$DEPLOY_SCRIPT" "$@" 2>&1)"
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

test_fresh_on_mainnet_without_confirmation_refused() {
    run_deploy --network mainnet --fresh --dry-run
    [[ $STATUS -ne 0 ]] && grep -q "Refusing mainnet deployment" <<< "$OUTPUT"
}

test_mainnet_fresh_with_network_but_without_second_flag_refused() {
    run_deploy --network mainnet --fresh --confirm-network mainnet --dry-run
    [[ $STATUS -ne 0 ]] && grep -q "Refusing --fresh on mainnet" <<< "$OUTPUT"
}

test_mainnet_fresh_with_wrong_confirmation_refused() {
    run_deploy --network mainnet --fresh --confirm-network yes --allow-mainnet-fresh --dry-run
    [[ $STATUS -ne 0 ]] && grep -q "Refusing mainnet deployment" <<< "$OUTPUT"
}

test_mainnet_fresh_with_both_flags_passes_guard() {
    run_deploy --network mainnet --fresh --confirm-network mainnet --allow-mainnet-fresh --dry-run
    ! grep -q "Refusing" <<< "$OUTPUT"
}

test_mainnet_without_fresh_needs_only_confirmation() {
    run_deploy --network mainnet --confirm-network mainnet --dry-run
    ! grep -q "Refusing" <<< "$OUTPUT"
}

test_mainnet_without_fresh_and_no_confirmation_refused() {
    run_deploy --network mainnet --dry-run
    [[ $STATUS -ne 0 ]] && grep -q "Refusing mainnet deployment" <<< "$OUTPUT"
}

test_testnet_fresh_not_affected_by_guard() {
    run_deploy --network testnet --fresh --dry-run
    ! grep -q "Refusing" <<< "$OUTPUT"
}

check "--fresh on mainnet without --confirm-network is refused" test_fresh_on_mainnet_without_confirmation_refused
check "--fresh on mainnet without --allow-mainnet-fresh is refused" test_mainnet_fresh_with_network_but_without_second_flag_refused
check "wrong --confirm-network value is refused" test_mainnet_fresh_with_wrong_confirmation_refused
check "--fresh on mainnet with both flags passes the guard" test_mainnet_fresh_with_both_flags_passes_guard
check "mainnet without --fresh needs only the confirmation" test_mainnet_without_fresh_needs_only_confirmation
check "mainnet without confirmation is refused" test_mainnet_without_fresh_and_no_confirmation_refused
check "testnet --fresh is not affected by the guard" test_testnet_fresh_not_affected_by_guard

if [[ $failures -gt 0 ]]; then
    echo "$failures check(s) failed"
    exit 1
fi
echo "All deploy fresh guard checks passed"
