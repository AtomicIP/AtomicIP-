#!/usr/bin/env bash

################################################################################
# Tests for Issue #919 / #1334: Deployment Configuration Validation
#
# These tests source scripts/deploy.sh and call its validate_config function
# directly with crafted config values. A check passes only if the real
# validation in deploy.sh accepts or rejects the value.
#
# Run with: ./scripts/test_deploy_config_validation.sh
################################################################################

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_SCRIPT="$SCRIPT_DIR/deploy.sh"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

test_count=0
pass_count=0
fail_count=0

# Load deploy.sh functions without running its main flow.
# deploy.sh uses LOG_FILE for logging, so give it a sink.
LOG_FILE=/dev/null
# shellcheck source=deploy.sh
source "$DEPLOY_SCRIPT"
set +e

# Run validate_config with the given assignments in a clean subshell.
# Usage: run_validation VAR=value ...; returns validate_config's exit status.
run_validation() {
    (
        unset TREASURY_ADDRESS ADMIN_PUBLIC_KEY NOTARY_PUBLIC_KEY
        for assignment in "$@"; do
            export "$assignment"
        done
        validate_config >/dev/null 2>&1
    )
}

expect_rejected() {
    local description=$1
    shift
    test_count=$((test_count + 1))
    echo -e "${BLUE}[TEST]${NC} $description"
    if run_validation "$@"; then
        echo -e "${RED}✗ FAIL${NC} config was accepted: $*"
        fail_count=$((fail_count + 1))
    else
        echo -e "${GREEN}✓ PASS${NC}"
        pass_count=$((pass_count + 1))
    fi
}

expect_accepted() {
    local description=$1
    shift
    test_count=$((test_count + 1))
    echo -e "${BLUE}[TEST]${NC} $description"
    if run_validation "$@"; then
        echo -e "${GREEN}✓ PASS${NC}"
        pass_count=$((pass_count + 1))
    else
        echo -e "${RED}✗ FAIL${NC} config was rejected: $*"
        fail_count=$((fail_count + 1))
    fi
}

main() {
    echo ""
    echo -e "${BLUE}=== Configuration Validation Tests ===${NC}"
    echo ""

    expect_rejected "Zero hex treasury address is rejected" \
        "TREASURY_ADDRESS=0x0000000000000000000000000000000000000000000000000000000000000000"
    expect_rejected "Zero Stellar treasury address is rejected" \
        "TREASURY_ADDRESS=GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
    expect_rejected "Zero notary public key is rejected" \
        "NOTARY_PUBLIC_KEY=0x0000000000000000000000000000000000000000000000000000000000000000"
    expect_rejected "Placeholder treasury address is rejected" \
        "TREASURY_ADDRESS=TREASURY_ADDRESS_PLACEHOLDER"
    expect_rejected "Placeholder admin key is rejected" \
        "ADMIN_PUBLIC_KEY=ADMIN_KEY_PLACEHOLDER"
    expect_rejected "Placeholder notary key is rejected" \
        "NOTARY_PUBLIC_KEY=NOTARY_KEY_PLACEHOLDER"
    expect_rejected "Zero admin key is rejected when another value is valid" \
        "ADMIN_PUBLIC_KEY=GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA" \
        "TREASURY_ADDRESS=GCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCY5V3A"
    expect_accepted "Valid treasury, admin and notary values are accepted" \
        "TREASURY_ADDRESS=GCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCY5V3A" \
        "ADMIN_PUBLIC_KEY=GBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBY5V3A" \
        "NOTARY_PUBLIC_KEY=GDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDY5V3A"
    expect_accepted "Unset optional values are not required" \
        "STELLAR_NETWORK=testnet"

    echo ""
    echo -e "${BLUE}=== Test Results ===${NC}"
    echo "Total: $test_count | Passed: $pass_count | Failed: $fail_count"

    if [[ $fail_count -eq 0 ]]; then
        echo -e "${GREEN}All tests passed!${NC}"
        exit 0
    else
        echo -e "${RED}Some tests failed!${NC}"
        exit 1
    fi
}

main
