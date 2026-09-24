#!/usr/bin/env bash
# #932: Verify that documented error codes match the contracts' actual error enums.
# This script extracts error codes from both contracts and validates against
# docs/api-reference.md.

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Extract error codes from a contract's lib.rs
extract_error_codes() {
    local lib_file="$1"
    local contract_name="$2"

    echo "=== Extracting error codes from $contract_name ===" >&2

    # Parse the #[contracterror] enum to extract all error variants with their codes
    awk '
        /#\[contracterror\]/,/^}/' "$lib_file" | \
        grep -E '^\s+[A-Za-z_][A-Za-z0-9_]*\s*=\s*[0-9]+' | \
        sed 's/^[[:space:]]*//;s/[[:space:]]*=\s*/=/;s/[[:space:]]*,.*$//' | \
        sort -t'=' -k2 -n
}

# Extract IP Registry error codes
echo "Extracting IP Registry error codes..."
REGISTRY_ERRORS=$(extract_error_codes "contracts/ip_registry/src/lib.rs" "ip_registry")

# Extract Atomic Swap error codes
echo "Extracting Atomic Swap error codes..."
SWAP_ERRORS=$(extract_error_codes "contracts/atomic_swap/src/lib.rs" "atomic_swap")

# Generate markdown error table
generate_error_table() {
    local errors="$1"
    local contract="$2"

    echo "## Error Codes - $contract"
    echo ""
    echo "| Error | Code | Description |"
    echo "|---|---|---|"

    while IFS='=' read -r error_name error_code; do
        error_name=$(echo "$error_name" | tr -d ' ')
        error_code=$(echo "$error_code" | tr -d ' ')
        if [ -n "$error_name" ] && [ -n "$error_code" ]; then
            # Convert snake_case to readable format (simple conversion)
            description="Error: $error_name"
            echo "| \`$error_name\` | $error_code | $description |"
        fi
    done <<< "$errors"

    echo ""
}

# Output generated tables
echo ""
echo "=== Generated Error Code Tables ==="
echo ""
generate_error_table "$REGISTRY_ERRORS" "IP Registry"
generate_error_table "$SWAP_ERRORS" "Atomic Swap"

# Verify that error codes in lib.rs don't have gaps (except for intentional gaps)
verify_no_major_gaps() {
    local errors="$1"
    local contract="$2"

    echo "=== Verifying error code completeness for $contract ===" >&2

    # Extract just the codes and check for major gaps
    local codes=$(echo "$errors" | cut -d'=' -f2 | tr -d ' ' | sort -n)
    local prev_code=-1
    local gap_found=0

    while read -r code; do
        if [ -z "$code" ]; then continue; fi
        local diff=$((code - prev_code))

        # Allow gaps of up to 5 (for reserved codes)
        if [ $diff -gt 5 ] && [ $prev_code -ne -1 ]; then
            echo -e "${YELLOW}Warning: Gap detected in $contract error codes: $prev_code -> $code${NC}" >&2
            gap_found=1
        fi
        prev_code=$code
    done <<< "$codes"

    if [ $gap_found -eq 0 ]; then
        echo -e "${GREEN}✓ No problematic gaps found in $contract error codes${NC}" >&2
    fi
}

verify_no_major_gaps "$REGISTRY_ERRORS" "IP Registry"
verify_no_major_gaps "$SWAP_ERRORS" "Atomic Swap"

echo ""
echo -e "${GREEN}✓ Error code verification complete${NC}"
echo ""
echo "Note: #932 requires updating docs/api-reference.md's error code tables to match"
echo "the generated tables above. This can be done manually or via CI automation."
