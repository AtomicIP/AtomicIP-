#!/bin/bash
# verify-all-services.sh
# Comprehensive service verification

set -e

API_URL="${API_URL:-https://api.atomicip.io}"
NETWORK="${NETWORK:-mainnet}"

echo "=== Verifying All Services ==="
echo ""

# Check API health
echo "Checking API health..."
if curl -sf "$API_URL/health" > /dev/null; then
    echo "✓ API is healthy"
else
    echo "✗ API is not responding"
    exit 1
fi

# Check API endpoints
echo ""
echo "Checking API endpoints..."
ENDPOINTS=(
    "/api/v1/ips"
    "/api/v1/swaps"
    "/api/v1/stats"
)

for endpoint in "${ENDPOINTS[@]}"; do
    if curl -sf "$API_URL$endpoint" > /dev/null; then
        echo "✓ $endpoint"
    else
        echo "✗ $endpoint"
        exit 1
    fi
done

# Check contract status
echo ""
echo "Checking contract status..."
if [ -n "$IP_REGISTRY_CONTRACT_ID" ]; then
    if stellar-cli contract invoke \
        --id "$IP_REGISTRY_CONTRACT_ID" \
        --network "$NETWORK" \
        -- get_ip_count > /dev/null 2>&1; then
        echo "✓ IP Registry contract is accessible"
    else
        echo "✗ IP Registry contract is not accessible"
        exit 1
    fi
fi

if [ -n "$ATOMIC_SWAP_CONTRACT_ID" ]; then
    if stellar-cli contract invoke \
        --id "$ATOMIC_SWAP_CONTRACT_ID" \
        --network "$NETWORK" \
        -- get_swap_count > /dev/null 2>&1; then
        echo "✓ Atomic Swap contract is accessible"
    else
        echo "✗ Atomic Swap contract is not accessible"
        exit 1
    fi
fi

# Check database connectivity
echo ""
echo "Checking database connectivity..."
if pg_isready -h "${DB_HOST:-localhost}" -p "${DB_PORT:-5432}" > /dev/null 2>&1; then
    echo "✓ Database is accessible"
else
    echo "✗ Database is not accessible"
    exit 1
fi

echo ""
echo "=== All Services Verified Successfully ==="
