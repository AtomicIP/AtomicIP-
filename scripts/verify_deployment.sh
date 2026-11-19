#!/usr/bin/env bash

################################################################################
# Verify the IP Registry admin is set on a deployed contract (#1259)
#
# The registry has no public admin getter. Instead this script simulates
# `initialize` with --send=no, which submits nothing. An initialized contract
# rejects it with AlreadyInitialized (error 37). If the simulation succeeds,
# the admin was never set and the deployment is broken.
#
# Usage: ./scripts/verify_deployment.sh <IP_REGISTRY_CONTRACT_ID> <NETWORK>
# Exit: 0 admin is set, 1 admin is missing or the check could not run
################################################################################

set -u

CONTRACT_ID="${1:-}"
NETWORK="${2:-}"

if [[ -z "$CONTRACT_ID" || -z "$NETWORK" ]]; then
    echo "usage: $0 <IP_REGISTRY_CONTRACT_ID> <NETWORK>" >&2
    exit 1
fi

if ! command -v stellar &> /dev/null; then
    echo "stellar CLI not found" >&2
    exit 1
fi

deployer_address="$(stellar keys address deployer 2>/dev/null)" || {
    echo "could not resolve deployer address" >&2
    exit 1
}

# Simulation only. Capture output and status separately.
output="$(stellar contract invoke \
    --id "$CONTRACT_ID" \
    --source deployer \
    --network "$NETWORK" \
    --send=no \
    -- initialize \
    --admin "$deployer_address" 2>&1)"
status=$?

if [[ $status -eq 0 ]]; then
    echo "FAIL: IP Registry $CONTRACT_ID accepted initialize; admin is not set" >&2
    exit 1
fi

# Error 37 is AlreadyInitialized in contracts/ip_registry (ContractError).
if grep -qE "AlreadyInitialized|#37\b" <<< "$output"; then
    echo "PASS: IP Registry $CONTRACT_ID admin is set"
    exit 0
fi

echo "FAIL: admin check could not confirm initialization: $output" >&2
exit 1
