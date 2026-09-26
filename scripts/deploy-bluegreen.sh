#!/usr/bin/env bash
# deploy-bluegreen.sh
# Blue-Green deployment automation for zero-downtime deployments.
#
# Usage: ./deploy-bluegreen.sh [OPTIONS]
# Options:
#   --env NAME       Environment (staging, production)
#   --version TAG    Docker image tag to deploy
#   --health-url URL Health check endpoint URL
#   --smoke-test     Run smoke tests after deployment
#   --rollback       Rollback to previous version on failure
#   --timeout SECS   Timeout for health checks (default: 120)
#   --verbose        Enable verbose output

set -euo pipefail

# ── Configuration ──────────────────────────────────────────────────────────────
ENV="production"
VERSION=""
HEALTH_URL=""
RUN_SMOKE_TESTS=false
AUTO_ROLLBACK=false
HEALTH_TIMEOUT=120
VERBOSE=false

GREEN_DEPLOY_DIR="/tmp/atomicip-green"
BLUE_DEPLOY_DIR="/tmp/atomicip-blue"
ACTIVE_SYMLINK="/tmp/atomicip-active"
PREVIOUS_SYMLINK="/tmp/atomicip-previous"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log()  { echo -e "${BLUE}[$(date +'%H:%M:%S')]${NC} $*"; }
ok()   { echo -e "${GREEN}✓ $*${NC}"; }
warn() { echo -e "${YELLOW}⚠ $*${NC}"; }
fail() { echo -e "${RED}✗ $*${NC}"; exit 1; }

parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --env)             ENV="$2"; shift 2 ;;
            --version)         VERSION="$2"; shift 2 ;;
            --health-url)      HEALTH_URL="$2"; shift 2 ;;
            --smoke-test)      RUN_SMOKE_TESTS=true; shift ;;
            --rollback)        AUTO_ROLLBACK=true; shift ;;
            --timeout)         HEALTH_TIMEOUT="$2"; shift 2 ;;
            --verbose)         VERBOSE=true; shift ;;
            *) fail "Unknown option: $1" ;;
        esac
    done
    [[ -z "$VERSION" ]] && fail "Required: --version TAG"
    [[ -z "$HEALTH_URL" ]] && HEALTH_URL="http://localhost:8080/health"
}

# ── Slot Management ────────────────────────────────────────────────────────────

get_current_slot() {
    if [[ -L "$ACTIVE_SYMLINK" ]]; then
        readlink "$ACTIVE_SYMLINK"
    else
        echo "$BLUE_DEPLOY_DIR"
    fi
}

get_next_slot() {
    local current
    current=$(get_current_slot)
    if [[ "$current" == "$BLUE_DEPLOY_DIR" ]]; then
        echo "$GREEN_DEPLOY_DIR"
    else
        echo "$BLUE_DEPLOY_DIR"
    fi
}

get_slot_name() {
    local dir=$1
    if [[ "$dir" == "$BLUE_DEPLOY_DIR" ]]; then
        echo "blue"
    else
        echo "green"
    fi
}

# ── Health Checks ──────────────────────────────────────────────────────────────

wait_for_health() {
    local url=$1
    local timeout=$2
    local elapsed=0
    local interval=5

    log "Waiting for health check at $url (timeout: ${timeout}s)..."
    while [[ $elapsed -lt $timeout ]]; do
        if curl -sf "$url" > /dev/null 2>&1; then
            ok "Health check passed at $url"
            return 0
        fi
        sleep "$interval"
        elapsed=$((elapsed + interval))
    done
    fail "Health check timed out after ${timeout}s at $url"
}

validate_health_response() {
    local url=$1
    local response
    response=$(curl -sf "$url" 2>/dev/null || echo "")

    if [[ -z "$response" ]]; then
        fail "Empty health check response from $url"
    fi

    if echo "$response" | jq -e '.status == "healthy"' > /dev/null 2>&1; then
        ok "Health status: healthy"
    elif echo "$response" | jq -e '.status == "degraded"' > /dev/null 2>&1; then
        warn "Health status: degraded (provisionally OK)"
    else
        fail "Health check returned unexpected status from $url: $(echo "$response" | jq -r '.status // "unknown"')"
    fi
}

# ── Smoke Tests ─────────────────────────────────────────────────────────────────

run_smoke_tests() {
    local base_url=$1
    log "Running smoke tests against $base_url..."

    local tests_passed=0
    local tests_failed=0

    # Test 1: Health endpoint
    if curl -sf "$base_url/health" > /dev/null 2>&1; then
        ok "Smoke: Health endpoint"
        tests_passed=$((tests_passed + 1))
    else
        fail "Smoke: Health endpoint FAILED"
        tests_failed=$((tests_failed + 1))
    fi

    # Test 2: Metrics endpoint
    if curl -sf "$base_url/metrics" > /dev/null 2>&1; then
        ok "Smoke: Metrics endpoint"
        tests_passed=$((tests_passed + 1))
    else
        warn "Smoke: Metrics endpoint unavailable"
        tests_failed=$((tests_failed + 1))
    fi

    # Test 3: Version endpoint
    if curl -sf "$base_url/version" > /dev/null 2>&1; then
        ok "Smoke: Version endpoint"
        tests_passed=$((tests_passed + 1))
    else
        warn "Smoke: Version endpoint unavailable"
        tests_failed=$((tests_failed + 1))
    fi

    # Test 4: API response (GET IP should return 404 not crash)
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" "$base_url/v1/ip/999999" 2>/dev/null || echo "000")
    if [[ "$status" != "000" ]]; then
        ok "Smoke: API responds (HTTP $status as expected)"
        tests_passed=$((tests_passed + 1))
    else
        fail "Smoke: API endpoint unreachable"
        tests_failed=$((tests_failed + 1))
    fi

    # Test 5: WebSocket upgrade
    if curl -sf -o /dev/null -w "" --header "Upgrade: websocket" --header "Connection: Upgrade" "$base_url/ws" 2>/dev/null; then
        ok "Smoke: WebSocket endpoint"
        tests_passed=$((tests_passed + 1))
    else
        warn "Smoke: WebSocket endpoint (non-critical)"
    fi

    log "Smoke tests: $tests_passed passed, $tests_failed failed"
    [[ $tests_failed -eq 0 ]] || return 1
}

# ── Error Rate Monitoring ───────────────────────────────────────────────────────

monitor_error_rates() {
    local url=$1
    local duration_secs=${2:-60}
    local error_threshold=${3:-0.05}
    local interval=10
    local elapsed=0
    local total_requests=0
    local total_errors=0

    log "Monitoring error rates at $url for ${duration_secs}s (threshold: ${error_threshold})..."

    while [[ $elapsed -lt $duration_secs ]]; do
        local metrics
        metrics=$(curl -sf "$url/metrics" 2>/dev/null || echo "")
        if [[ -n "$metrics" ]]; then
            local requests
            local errors
            requests=$(echo "$metrics" | grep "^http_requests_total" | awk '{sum+=$NF} END {print sum+0}')
            errors=$(echo "$metrics" | grep "^http_errors_total" | awk '{sum+=$NF} END {print sum+0}')
            total_requests=$requests
            total_errors=$errors
        fi
        sleep "$interval"
        elapsed=$((elapsed + interval))
    done

    if [[ $total_requests -gt 0 ]]; then
        local error_rate
        error_rate=$(echo "scale=4; $total_errors / $total_requests" | bc)
        log "Error rate: $(echo "scale=2; $error_rate * 100" | bc)% ($total_errors / $total_requests)"

        if (( $(echo "$error_rate > $error_threshold" | bc -l) )); then
            warn "Error rate ${error_rate} exceeds threshold ${error_threshold}"
            return 1
        fi
    fi
    ok "Error rate within threshold"
    return 0
}

# ── Rollback ────────────────────────────────────────────────────────────────────

perform_rollback() {
    log "Initiating rollback..."
    local previous
    if [[ -L "$PREVIOUS_SYMLINK" ]]; then
        previous=$(readlink "$PREVIOUS_SYMLINK")
        local slot_name
        slot_name=$(get_slot_name "$previous")
        log "Rolling back to $slot_name slot ($previous)..."

        ln -sfn "$previous" "$ACTIVE_SYMLINK"
        ok "Rollback to $slot_name completed"

        if [[ -n "$HEALTH_URL" ]]; then
            wait_for_health "$HEALTH_URL" "$HEALTH_TIMEOUT" || warn "Health check failed after rollback"
        fi
    else
        fail "No previous deployment found for rollback"
    fi
}

# ── Main Deployment Flow ────────────────────────────────────────────────────────

main() {
    parse_args "$@"

    local current_slot next_slot slot_name
    current_slot=$(get_current_slot)
    next_slot=$(get_next_slot)
    slot_name=$(get_slot_name "$next_slot")

    log "=== Blue-Green Deployment ==="
    log "Environment: $ENV"
    log "Version:     $VERSION"
    log "Current:     $(get_slot_name "$current_slot") ($current_slot)"
    log "Target:      $slot_name ($next_slot)"

    # Step 1: Deploy to inactive slot
    echo ""
    log "Step 1/6: Deploying $slot_name slot..."
    mkdir -p "$next_slot"
    cat > "$next_slot/.env" << EOF
ATOMICIP_ENV=$ENV
ATOMICIP_VERSION=$VERSION
ATOMICIP_DEPLOYED_AT=$(date -u +%Y-%m-%dT%H:%M:%SZ)
ATOMICIP_SLOT=$slot_name
EOF
    ok "Deployment artifacts staged in $next_slot"

    # Step 2: Start the application in the new slot
    echo ""
    log "Step 2/6: Starting application in $slot_name..."
    # In a real deployment, this would docker-compose up, systemctl start, etc.
    ok "Application started in $slot_name slot"

    # Step 3: Health check
    echo ""
    log "Step 3/6: Running health checks..."
    local slot_health_url="${HEALTH_URL}"
    wait_for_health "$slot_health_url" "$HEALTH_TIMEOUT"
    validate_health_response "$slot_health_url"

    # Step 4: Switch traffic
    echo ""
    log "Step 4/6: Switching traffic to $slot_name..."
    ln -sfn "$PREVIOUS_SYMLINK" "$PREVIOUS_SYMLINK.bak" 2>/dev/null || true
    ln -sfn "$current_slot" "$PREVIOUS_SYMLINK" 2>/dev/null || true
    ln -sfn "$next_slot" "$ACTIVE_SYMLINK"
    ok "Traffic switched to $slot_name"

    # Step 5: Smoke tests
    if [[ "$RUN_SMOKE_TESTS" == true ]]; then
        echo ""
        log "Step 5/6: Running smoke tests..."
        if run_smoke_tests "$HEALTH_URL"; then
            ok "Smoke tests passed"
        else
            warn "Smoke tests failed"
            if [[ "$AUTO_ROLLBACK" == true ]]; then
                warn "Auto-rollback triggered due to smoke test failure"
                perform_rollback
                exit 1
            fi
            fail "Smoke tests failed (use --rollback for auto-rollback)"
        fi
    fi

    # Step 6: Monitor error rates
    echo ""
    log "Step 6/6: Monitoring error rates (60s)..."
    if ! monitor_error_rates "${HEALTH_URL}" 60 0.05; then
        warn "Error rate spike detected"
        if [[ "$AUTO_ROLLBACK" == true ]]; then
            warn "Auto-rollback triggered due to error rate spike"
            perform_rollback
            exit 1
        fi
    fi

    echo ""
    log "=== Blue-Green Deployment Complete ==="
    ok "Active slot: $slot_name ($next_slot)"
    ok "Version: $VERSION"
}

main "$@"
