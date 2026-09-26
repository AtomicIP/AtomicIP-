#!/usr/bin/env bash
# rollback.sh
# Automatic rollback trigger for blue-green deployment.
# Monitors error rate and triggers rollback if threshold exceeded.
#
# Usage: ./rollback.sh --url URL [--threshold RATE] [--duration SECS]

set -euo pipefail

URL=""
ERROR_THRESHOLD=0.05
DURATION=60

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log() { echo -e "${BLUE}[$(date +'%H:%M:%S')]${NC} $*"; }
ok()  { echo -e "${GREEN}✓ $*${NC}"; }
warn(){ echo -e "${YELLOW}⚠ $*${NC}"; }
fail(){ echo -e "${RED}✗ $*${NC}"; exit 1; }

parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --url)       URL="$2"; shift 2 ;;
            --threshold) ERROR_THRESHOLD="$2"; shift 2 ;;
            --duration)  DURATION="$2"; shift 2 ;;
            *) fail "Unknown option: $1" ;;
        esac
    done
    [[ -z "$URL" ]] && fail "Required: --url URL"
}

check_active_slot() {
    local active
    if [[ -L "/tmp/atomicip-active" ]]; then
        active=$(readlink "/tmp/atomicip-active")
        log "Active slot: $(basename "$active")"
    else
        warn "No active deployment symlink found"
        return 1
    fi
}

perform_rollback() {
    log "=== Initiating Rollback ==="
    local previous
    if [[ -L "/tmp/atomicip-previous" ]]; then
        previous=$(readlink "/tmp/atomicip-previous")
        local slot_name
        slot_name=$(basename "$previous")
        log "Rolling back to $slot_name ($previous)..."
        ln -sfn "$previous" "/tmp/atomicip-active"
        ok "Rollback to $slot_name completed"
    else
        fail "No previous deployment found for rollback"
    fi
}

monitor_and_rollback() {
    log "Monitoring error rate at $URL (threshold: ${ERROR_THRESHOLD}, duration: ${DURATION}s)..."
    local interval=10
    local elapsed=0

    while [[ $elapsed -lt $DURATION ]]; do
        local metrics
        metrics=$(curl -sf "$URL/metrics" 2>/dev/null || echo "")

        if [[ -n "$metrics" ]]; then
            local requests errors error_rate
            requests=$(echo "$metrics" | grep "^http_requests_total" | awk '{sum+=$NF} END {print sum+0}')
            errors=$(echo "$metrics" | grep "^http_errors_total" | awk '{sum+=$NF} END {print sum+0}')

            if [[ $requests -gt 0 ]]; then
                error_rate=$(echo "scale=6; $errors / $requests" | bc 2>/dev/null || echo "0")
                local pct
                pct=$(echo "scale=2; $error_rate * 100" | bc)
                log "Error rate: ${pct}% ($errors / $requests)"

                if (( $(echo "$error_rate > $ERROR_THRESHOLD" | bc -l 2>/dev/null || echo "0") )); then
                    warn "Error rate ${pct}% exceeds threshold $(echo "$ERROR_THRESHOLD * 100" | bc)%"
                    perform_rollback
                    return 1
                fi
            fi
        fi

        sleep "$interval"
        elapsed=$((elapsed + interval))
    done

    ok "Error rate within threshold for ${DURATION}s — no rollback needed"
    return 0
}

main() {
    parse_args "$@"
    log "=== Rollback Monitor ==="
    check_active_slot
    monitor_and_rollback
}

main "$@"
