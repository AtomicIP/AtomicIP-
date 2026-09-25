#!/usr/bin/env bash
# #932: CI check that fails if docs/api-reference.md's error tables have drifted
# from the contracts' actual error enums.
#
# This script:
# 1. Extracts error codes from both contracts' lib.rs files
# 2. Compares against documented error codes in docs/api-reference.md
# 3. Fails the build if there's a drift

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

DRIFT_FOUND=0

# Extract error codes from contract lib.rs
extract_contract_errors() {
    local lib_file="$1"

    # Extract from #[contracterror] enum
    awk '
        /#\[contracterror\]/,/^}/' "$lib_file" | \
        grep -E '^\s+[A-Za-z_][A-Za-z0-9_]*\s*=\s*[0-9]+' | \
        sed 's/^[[:space:]]*//;s/[[:space:]]*=\s*/=/;s/[[:space:]]*[,\/].*$//' | \
        sort -t'=' -k2 -n
}

# Extract documented error codes from api-reference.md
extract_documented_errors() {
    local doc_file="$1"

    # Extract error codes from markdown tables with pattern: | `ErrorName` | code |
    grep '| `[A-Za-z_][A-Za-z0-9_]*` | [0-9]' "$doc_file" | \
        sed 's/.*| `\([^`]*\)` | \([0-9]*\).*/\1=\2/' | \
        sort -t'=' -k2 -n
}

# Compare two error code lists
compare_errors() {
    local contract_name="$1"
    local contract_errors="$2"
    local documented_errors="$3"

    echo "=== Checking $contract_name error codes ===" >&2

    local contract_codes=$(echo "$contract_errors" | cut -d'=' -f2 | sort -n | tr '\n' ' ')
    local documented_codes=$(echo "$documented_errors" | cut -d'=' -f2 | sort -n | tr '\n' ' ')

    if [ "$contract_codes" != "$documented_codes" ]; then
        echo -e "${RED}✗ DRIFT DETECTED in $contract_name error codes!${NC}" >&2
        echo "  Contract codes:     $contract_codes" >&2
        echo "  Documented codes:   $documented_codes" >&2
        return 1
    fi

    # Check for code mismatches
    while IFS='=' read -r name code; do
        if [ -z "$name" ] || [ -z "$code" ]; then continue; fi

        # Check if this error is documented
        if ! echo "$documented_errors" | grep -q "^${name}=${code}$"; then
            echo -e "${RED}✗ Error '$name' (code $code) is in $contract_name but not documented!${NC}" >&2
            return 1
        fi
    done <<< "$contract_errors"

    return 0
}

# Main verification
main() {
    echo "Verifying error code synchronization..." >&2
    echo ""

    # Extract IP Registry errors
    echo "Checking IP Registry..." >&2
    REGISTRY_CONTRACT=$(extract_contract_errors "contracts/ip_registry/src/lib.rs")
    REGISTRY_DOCUMENTED=$(extract_documented_errors "docs/api-reference.md")

    if ! compare_errors "IP Registry" "$REGISTRY_CONTRACT" "$REGISTRY_DOCUMENTED"; then
        DRIFT_FOUND=1
    fi

    # Extract Atomic Swap errors (if documented in api-reference.md)
    echo "Checking Atomic Swap..." >&2
    SWAP_CONTRACT=$(extract_contract_errors "contracts/atomic_swap/src/lib.rs")
    # Note: Atomic Swap error documentation might be in a separate file or section
    # For now, we'll check if there's a dedicated section

    if grep -q "Atomic Swap" docs/api-reference.md; then
        SWAP_DOCUMENTED=$(extract_documented_errors "docs/api-reference.md")
        if ! compare_errors "Atomic Swap" "$SWAP_CONTRACT" "$SWAP_DOCUMENTED"; then
            DRIFT_FOUND=1
        fi
    else
        echo -e "${YELLOW}⚠ Atomic Swap error codes not found in docs/api-reference.md${NC}" >&2
        echo "  Run 'scripts/verify-error-codes.sh' to see all contract error codes" >&2
        DRIFT_FOUND=1
    fi

    echo ""

    if [ $DRIFT_FOUND -eq 0 ]; then
        echo -e "${GREEN}✓ All error codes are synchronized${NC}" >&2
        return 0
    else
        echo -e "${RED}✗ Error code drift detected! Run 'scripts/verify-error-codes.sh' for details.${NC}" >&2
        echo "  Then update docs/api-reference.md to match the contracts' actual error enums." >&2
        return 1
    fi
}

main "$@"
