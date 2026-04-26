#!/bin/bash
# smoke-test.sh
# Quick smoke tests for critical functionality

set -e

API_URL="${API_URL:-https://api.atomicip.io}"
NETWORK="${NETWORK:-testnet}"

echo "=== Running Smoke Tests ==="
echo ""

# Test 1: IP Registration
echo "Test 1: IP Registration..."
RESPONSE=$(curl -sf -X POST "$API_URL/api/v1/ips" \
    -H "Content-Type: application/json" \
    -d '{
        "owner": "test_owner",
        "commitment_hash": "0000000000000000000000000000000000000000000000000000000000000001"
    }' || echo "FAILED")

if [ "$RESPONSE" != "FAILED" ]; then
    echo "✓ IP registration works"
else
    echo "✗ IP registration failed"
    exit 1
fi

# Test 2: IP Retrieval
echo ""
echo "Test 2: IP Retrieval..."
if curl -sf "$API_URL/api/v1/ips/1" > /dev/null; then
    echo "✓ IP retrieval works"
else
    echo "✗ IP retrieval failed"
    exit 1
fi

# Test 3: Swap Initiation
echo ""
echo "Test 3: Swap Initiation..."
RESPONSE=$(curl -sf -X POST "$API_URL/api/v1/swaps" \
    -H "Content-Type: application/json" \
    -d '{
        "ip_id": 1,
        "price": 1000,
        "buyer": "test_buyer"
    }' || echo "FAILED")

if [ "$RESPONSE" != "FAILED" ]; then
    echo "✓ Swap initiation works"
else
    echo "✗ Swap initiation failed"
    exit 1
fi

# Test 4: Stats Endpoint
echo ""
echo "Test 4: Stats Endpoint..."
if curl -sf "$API_URL/api/v1/stats" > /dev/null; then
    echo "✓ Stats endpoint works"
else
    echo "✗ Stats endpoint failed"
    exit 1
fi

echo ""
echo "=== All Smoke Tests Passed ==="
