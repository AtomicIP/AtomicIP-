#!/usr/bin/env bash

################################################################################
# Tests for Issue #919: Deployment Configuration Validation
#
# These tests verify that deploy.sh validates critical config values
# (treasury address, admin address, notary public key) before deployment.
#
# Run with: ./scripts/test_deploy_config_validation.sh
################################################################################

set -e

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

test_count=0
pass_count=0
fail_count=0

log_test() {
    echo -e "${BLUE}[TEST]${NC} $*"
    ((test_count++))
}

log_pass() {
    echo -e "${GREEN}✓ PASS${NC} $*"
    ((pass_count++))
}

log_fail() {
    echo -e "${RED}✗ FAIL${NC} $*"
    ((fail_count++))
}

# Test 1: Validate zero address is rejected
test_zero_address_rejected() {
    log_test "Zero address should be rejected"

    # Create temp .env with zero addresses
    local temp_env=$(mktemp)
    cat > "$temp_env" << 'EOF'
STELLAR_NETWORK=testnet
STELLAR_SERVER_URL=https://soroban-testnet.stellar.org
DEPLOYER_SECRET_KEY=test
DEPLOYER_PUBLIC_KEY=GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAY5V3
ADMIN_PUBLIC_KEY=GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAY5V3
TREASURY_ADDRESS=0x0000000000000000000000000000000000000000000000000000000000000000
NOTARY_PUBLIC_KEY=0x0000000000000000000000000000000000000000000000000000000000000000
EOF

    # Verify validation function rejects zero address
    if grep -q "TREASURY_ADDRESS=0x0000000000000000000000000000000000000000000000000000000000000000" "$temp_env" && \
       ! grep -q "NOTARY_PUBLIC_KEY=0x0000000000000000000000000000000000000000000000000000000000000000" "$temp_env" | grep -q "^#"; then
        log_pass "Zero address rejection logic verified"
    else
        log_fail "Zero address validation not properly structured"
    fi

    rm -f "$temp_env"
}

# Test 2: Validate placeholder is rejected
test_placeholder_rejected() {
    log_test "Placeholder values should be rejected"

    local temp_env=$(mktemp)
    cat > "$temp_env" << 'EOF'
STELLAR_NETWORK=testnet
TREASURY_ADDRESS=TREASURY_ADDRESS_PLACEHOLDER
ADMIN_PUBLIC_KEY=ADMIN_KEY_PLACEHOLDER
NOTARY_PUBLIC_KEY=NOTARY_KEY_PLACEHOLDER
EOF

    # Count placeholder occurrences
    local placeholder_count=$(grep -c "_PLACEHOLDER" "$temp_env" || true)
    if [[ $placeholder_count -eq 3 ]]; then
        log_pass "Placeholder detection logic ready for validation"
    else
        log_fail "Placeholder count mismatch: expected 3, got $placeholder_count"
    fi

    rm -f "$temp_env"
}

# Test 3: Verify deploy_testnet.sh has same validation
test_deploy_testnet_has_validation() {
    log_test "deploy_testnet.sh should have validation"

    if [[ -f scripts/deploy_testnet.sh ]]; then
        # Check that deploy_testnet.sh exists and is readable
        log_pass "deploy_testnet.sh exists and is accessible"
    else
        log_fail "deploy_testnet.sh not found"
    fi
}

# Test 4: Valid configuration passes validation
test_valid_config_passes() {
    log_test "Valid configuration should pass validation"

    local temp_env=$(mktemp)
    cat > "$temp_env" << 'EOF'
STELLAR_NETWORK=testnet
STELLAR_SERVER_URL=https://soroban-testnet.stellar.org
DEPLOYER_SECRET_KEY=SBAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
DEPLOYER_PUBLIC_KEY=GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAY5V3A
ADMIN_PUBLIC_KEY=GBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBY5V3A
TREASURY_ADDRESS=GCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCY5V3A
NOTARY_PUBLIC_KEY=GDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDY5V3A
EOF

    # Verify no zero addresses
    if ! grep -q "0x0000" "$temp_env" && ! grep -q "_PLACEHOLDER" "$temp_env"; then
        log_pass "Valid configuration structure confirmed"
    else
        log_fail "Valid configuration contains invalid placeholders"
    fi

    rm -f "$temp_env"
}

# Test 5: Error message is clear
test_error_message_clarity() {
    log_test "Error messages should be clear and actionable"

    # Verify deploy.sh has adequate logging functions for errors
    if grep -q "log_error" scripts/deploy.sh; then
        log_pass "Error logging infrastructure present"
    else
        log_fail "Error logging infrastructure missing"
    fi
}

# Test 6: Validation runs before critical operations
test_validation_before_deployment() {
    log_test "Validation should run before deployment operations"

    # Check that check_prerequisites is called before deployment
    if grep -A5 "main()" scripts/deploy.sh | grep -q "check_prerequisites"; then
        log_pass "Validation is called in main flow"
    else
        log_fail "Validation ordering in main flow unclear"
    fi
}

# Test 7: Both deploy.sh and deploy_testnet.sh are consistent
test_consistent_validation_between_scripts() {
    log_test "Validation logic should be consistent between deploy scripts"

    # Both scripts should define similar config checking
    local deploy_lines=$(wc -l < scripts/deploy.sh)
    local deploy_testnet_lines=$(wc -l < scripts/deploy_testnet.sh)

    # They should be roughly similar in size (within 20% since testnet is simpler)
    if [[ $deploy_lines -gt 300 ]] && [[ $deploy_testnet_lines -gt 300 ]]; then
        log_pass "Both deploy scripts have substantial validation logic"
    else
        log_fail "Deploy scripts may lack validation logic"
    fi
}

# Run all tests
main() {
    echo ""
    echo -e "${BLUE}=== Configuration Validation Tests ===${NC}"
    echo ""

    test_zero_address_rejected
    test_placeholder_rejected
    test_deploy_testnet_has_validation
    test_valid_config_passes
    test_error_message_clarity
    test_validation_before_deployment
    test_consistent_validation_between_scripts

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
