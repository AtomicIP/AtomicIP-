#!/usr/bin/env bash
# health-check.sh
# Validate API server health with detailed component checks.
# Used by blue-green deployment for pre/post-deployment validation.
#
# Usage: ./health-check.sh [URL] [--detailed]

set -euo pipefail

URL="${1:-http://localhost:8080}"
DETAILED=false
[[ "${2:-}" == "--detailed" ]] && DETAILED=true

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

ok()   { echo -e "${GREEN}✓ $*${NC}"; }
warn() { echo -e "${YELLOW}⚠ $*${NC}"; }
fail() { echo -e "${RED}✗ $*${NC}"; exit 1; }

echo "Health Check: $URL"
echo ""

# Basic health check
HEALTH_URL="$URL/health"
RESPONSE=$(curl -sf "$HEALTH_URL" 2>/dev/null || echo "")

if [[ -z "$RESPONSE" ]]; then
    fail "Cannot reach health endpoint at $HEALTH_URL"
fi

STATUS=$(echo "$RESPONSE" | jq -r '.status // "unknown"' 2>/dev/null)
UPTIME=$(echo "$RESPONSE" | jq -r '.uptime_seconds // "unknown"' 2>/dev/null)

if [[ "$STATUS" == "healthy" ]]; then
    ok "Overall status: healthy (uptime: ${UPTIME}s)"
elif [[ "$STATUS" == "degraded" ]]; then
    warn "Overall status: degraded"
else
    fail "Overall status: $STATUS"
fi

# Component checks
if [[ "$DETAILED" == true ]]; then
    echo ""
    echo "Component Health:"
    for component in contract_connectivity database cache memory disk; do
        COMP_STATUS=$(echo "$RESPONSE" | jq -r ".components.$component.status // \"unknown\"" 2>/dev/null)
        COMP_LATENCY=$(echo "$RESPONSE" | jq -r ".components.$component.latency_ms // \"0\"" 2>/dev/null)
        if [[ "$COMP_STATUS" == "healthy" ]]; then
            ok "${component}: ${COMP_STATUS} (${COMP_LATENCY}ms)"
        else
            warn "${component}: ${COMP_STATUS}"
        fi
    done
fi

# Metrics endpoint check
echo ""
if curl -sf "$URL/metrics" > /dev/null 2>&1; then
    ok "Metrics endpoint accessible"
else
    warn "Metrics endpoint not available"
fi

# API endpoint check
API_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$URL/v1/ip/1" 2>/dev/null || echo "000")
if [[ "$API_STATUS" != "000" ]]; then
    ok "API endpoint responds (HTTP $API_STATUS)"
else
    warn "API endpoint not reachable"
fi

echo ""
ok "Health check complete"
