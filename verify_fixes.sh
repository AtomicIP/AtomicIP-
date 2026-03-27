#!/bin/bash
# Verification script for the 4 GitHub issue fixes
# Run this when disk space is available

set -e

echo "======================================"
echo "AtomicIP Smart Contract Fixes Verification"
echo "======================================"
echo ""

echo "Checking disk space..."
AVAILABLE_SPACE=$(df -P . | awk 'NR==2 {print $4}')
if [ "$AVAILABLE_SPACE" -lt 5000000 ]; then
    echo "ERROR: Insufficient disk space (need at least 5GB)"
    echo "Current available: $((AVAILABLE_SPACE / 1024 / 1024))GB"
    exit 1
fi
echo "✓ Disk space OK: $((AVAILABLE_SPACE / 1024 / 1024))GB available"
echo ""

echo "Cleaning previous builds..."
cargo clean
echo ""

echo "Building atomic_swap contract..."
cargo build --package atomic_swap --lib
echo "✓ atomic_swap compiled successfully"
echo ""

echo "Building ip_registry contract..."
cargo build --package ip_registry --lib
echo "✓ ip_registry compiled successfully"
echo ""

echo "Running atomic_swap tests..."
cargo test --package atomic_swap
echo "✓ All atomic_swap tests passed"
echo ""

echo "Running ip_registry tests..."
cargo test --package ip_registry
echo "✓ All ip_registry tests passed"
echo ""

echo "======================================"
echo "All verifications passed!"
echo "======================================"
echo ""
echo "Fixed Issues:"
echo "  ✓ #32 - reveal_key verifies decryption key (pre-existing)"
echo "  ✓ #34 - reveal_key releases payment to seller"
echo "  ✓ #35 - cancel_expired_swap refunds buyer"
echo "  ✓ #44 - commit_ip rejects duplicate hashes (pre-existing)"
echo ""
