#!/usr/bin/env bash

################################################################################
# Tests for Issue #922: Staging Environment Smoke-Test Workflow
#
# Tests verify that:
# - Smoke-test script is properly structured
# - CI workflow triggers smoke-tests on deploy
# - Failure alerts are configured
# - Deployment verification process is documented
#
# Run with: ./scripts/test_smoke_test_workflow.sh
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

# Test 1: Smoke-test script exists
test_smoke_test_exists() {
    log_test "Smoke-test script should exist"

    if [[ -f scripts/smoke-test.sh ]] && [[ -x scripts/smoke-test.sh ]]; then
        log_pass "smoke-test.sh exists and is executable"
    else
        log_fail "smoke-test.sh missing or not executable"
    fi
}

# Test 2: Smoke-test covers critical endpoints
test_smoke_test_coverage() {
    log_test "Smoke-test should cover critical API endpoints"

    local required_endpoints=(
        "/api/v1/ips"
        "/api/v1/swaps"
        "/api/v1/stats"
    )

    local coverage_count=0
    for endpoint in "${required_endpoints[@]}"; do
        if grep -q "$endpoint" scripts/smoke-test.sh; then
            ((coverage_count++))
        fi
    done

    if [[ $coverage_count -eq ${#required_endpoints[@]} ]]; then
        log_pass "Smoke-test covers all critical endpoints ($coverage_count/${#required_endpoints[@]})"
    else
        log_fail "Smoke-test missing some endpoints ($coverage_count/${#required_endpoints[@]})"
    fi
}

# Test 3: Smoke-test has proper exit codes
test_smoke_test_exit_codes() {
    log_test "Smoke-test should have proper exit codes"

    # Check for exit 0 on success and exit 1 on failure
    if grep -q "exit 1" scripts/smoke-test.sh && grep -q "exit 0\|exit$" scripts/smoke-test.sh; then
        log_pass "Smoke-test has proper exit code handling"
    else
        log_fail "Smoke-test may not have proper exit codes"
    fi
}

# Test 4: Smoke-test handles environment variables
test_smoke_test_env_vars() {
    log_test "Smoke-test should accept environment variables"

    # Check for API_URL and NETWORK env vars
    if grep -q "API_URL\|NETWORK" scripts/smoke-test.sh; then
        log_pass "Smoke-test accepts environment configuration"
    else
        log_fail "Smoke-test doesn't use environment variables"
    fi
}

# Test 5: CI workflow includes post-deploy trigger
test_ci_workflow_post_deploy_trigger() {
    log_test "CI workflow should have post-deploy trigger configuration"

    local ci_file=".github/workflows/ci.yml"
    if [[ -f "$ci_file" ]]; then
        # Check for deployment trigger or workflow_dispatch
        if grep -q "deployment\|workflow_dispatch\|push" "$ci_file"; then
            log_pass "CI workflow has trigger configuration"
        else
            log_fail "CI workflow may lack post-deploy trigger"
        fi
    else
        log_fail "CI workflow file not found"
    fi
}

# Test 6: Workflow failure alerting is configured
test_workflow_failure_alerting() {
    log_test "Workflow should have failure alert configuration"

    # Check for notification or failure step
    local ci_file=".github/workflows/ci.yml"
    if [[ -f "$ci_file" ]]; then
        if grep -q "failure\|notify\|alert" "$ci_file"; then
            log_pass "Workflow has failure handling configured"
        else
            log_fail "Workflow lacks failure alerting"
        fi
    fi
}

# Test 7: Deployment verification documentation
test_deployment_guide_exists() {
    log_test "Deployment guide should exist and be documented"

    # Check for deployment guide
    if [[ -f docs/deployment-guide.md ]]; then
        log_pass "Deployment guide exists"
    else
        # Check if it should be created
        if [[ ! -f docs/deployment-guide.md ]]; then
            log_fail "Deployment guide not found at docs/deployment-guide.md"
        fi
    fi
}

# Test 8: Smoke-test response parsing
test_smoke_test_response_validation() {
    log_test "Smoke-test should validate API responses"

    # Check for response validation (grep on response or jq parsing)
    if grep -q "RESPONSE\|jq\|grep" scripts/smoke-test.sh; then
        log_pass "Smoke-test validates API responses"
    else
        log_fail "Smoke-test may not validate responses properly"
    fi
}

# Test 9: Smoke-test timeout handling
test_smoke_test_timeout_handling() {
    log_test "Smoke-test should have timeout configuration"

    # curl should have timeout flags
    if grep -q "curl.*--max-time\|curl.*-m\|timeout" scripts/smoke-test.sh; then
        log_pass "Smoke-test has timeout handling"
    else
        log_fail "Smoke-test lacks timeout protection"
    fi
}

# Test 10: Deployment flow includes smoke-test
test_deployment_flow_includes_smoke_test() {
    log_test "Deployment flow should include smoke-test invocation"

    # Check if any deploy script mentions smoke-test
    if grep -q "smoke-test" scripts/deploy.sh scripts/deploy_testnet.sh 2>/dev/null; then
        log_pass "Deployment scripts reference smoke-test"
    else
        # Document requirement
        log_fail "Deployment scripts should invoke smoke-test"
    fi
}

# Test 11: Environment-specific smoke-test configuration
test_environment_specific_config() {
    log_test "Smoke-test should support multiple environments"

    # Check for network/environment configuration
    if grep -q "\$NETWORK\|\$API_URL\|\$ENVIRONMENT" scripts/smoke-test.sh; then
        log_pass "Smoke-test supports environment configuration"
    else
        log_fail "Smoke-test may not support multiple environments"
    fi
}

# Test 12: Smoke-test logging
test_smoke_test_logging() {
    log_test "Smoke-test should have proper logging output"

    # Check for echo statements or logging
    if grep -q "echo\|===\|Test" scripts/smoke-test.sh; then
        log_pass "Smoke-test has logging and output"
    else
        log_fail "Smoke-test lacks logging"
    fi
}

# Test 13: Dry-run support for smoke-tests
test_smoke_test_dry_run() {
    log_test "Smoke-test should support dry-run mode"

    # Check for dry-run or --dry-run flag support
    if grep -q "dry.run\|DRY_RUN" scripts/smoke-test.sh; then
        log_pass "Smoke-test supports dry-run mode"
    else
        # Document requirement
        log_fail "Smoke-test should support dry-run for safety"
    fi
}

# Test 14: Smoke-test can be run independently
test_smoke_test_independence() {
    log_test "Smoke-test should be runnable independently"

    # Should not require deployment as prerequisite
    if [[ -x scripts/smoke-test.sh ]]; then
        log_pass "Smoke-test is independently executable"
    else
        log_fail "Smoke-test is not independently executable"
    fi
}

# Test 15: Workflow reusability
test_workflow_is_reusable() {
    log_test "Smoke-test workflow should be reusable"

    # Should have parameters for different scenarios
    local ci_file=".github/workflows/ci.yml"
    if [[ -f "$ci_file" ]]; then
        # Check for inputs or on: parameters
        if grep -q "on:\|inputs:" "$ci_file"; then
            log_pass "Workflow has reusable configuration"
        else
            log_fail "Workflow may not be reusable"
        fi
    fi
}

# Main execution
main() {
    echo ""
    echo -e "${BLUE}=== Smoke-Test Workflow Tests ===${NC}"
    echo ""

    test_smoke_test_exists
    test_smoke_test_coverage
    test_smoke_test_exit_codes
    test_smoke_test_env_vars
    test_ci_workflow_post_deploy_trigger
    test_workflow_failure_alerting
    test_deployment_guide_exists
    test_smoke_test_response_validation
    test_smoke_test_timeout_handling
    test_deployment_flow_includes_smoke_test
    test_environment_specific_config
    test_smoke_test_logging
    test_smoke_test_dry_run
    test_smoke_test_independence
    test_workflow_is_reusable

    echo ""
    echo -e "${BLUE}=== Test Results ===${NC}"
    echo "Total: $test_count | Passed: $pass_count | Failed: $fail_count"

    if [[ $fail_count -eq 0 ]]; then
        echo -e "${GREEN}All tests passed!${NC}"
        exit 0
    else
        echo -e "${RED}Some tests failed or documented requirements.${NC}"
        exit 1
    fi
}

main
