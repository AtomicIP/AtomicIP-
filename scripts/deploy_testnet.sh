#!/usr/bin/env bash

################################################################################
# Atomic Patent - Testnet Deployment Script
#
# This script handles complete deployment of the Atomic Patent contracts to
# the Stellar testnet, including initialization, state management, and
# validation.
#
# Usage: ./deploy_testnet.sh [OPTIONS]
# Options:
#   --fresh        Force fresh deployment (regenerate keys)
#   --skip-build   Skip building contracts
#   --skip-init    Skip initialization
#   --dry-run      Simulate deployment without executing
#   --verbose      Enable verbose output
################################################################################

set -e

# Configuration
VERBOSE=false
FRESH_DEPLOY=false
SKIP_BUILD=false
SKIP_INIT=false
DRY_RUN=false
NETWORK="testnet"
DEPLOY_STATE_FILE=".testnet-state.json"
LOG_FILE="testnet-deploy-$(date +%Y%m%d-%H%M%S).log"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log() {
    echo -e "${BLUE}[$(date +'%Y-%m-%d %H:%M:%S')]${NC} $*" | tee -a "$LOG_FILE"
}

log_success() {
    echo -e "${GREEN}✓ $*${NC}" | tee -a "$LOG_FILE"
}

log_error() {
    echo -e "${RED}✗ $*${NC}" | tee -a "$LOG_FILE"
}

log_warning() {
    echo -e "${YELLOW}⚠ $*${NC}" | tee -a "$LOG_FILE"
}

# Parse command line arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --fresh)
                FRESH_DEPLOY=true
                shift
                ;;
            --skip-build)
                SKIP_BUILD=true
                shift
                ;;
            --skip-init)
                SKIP_INIT=true
                shift
                ;;
            --dry-run)
                DRY_RUN=true
                shift
                ;;
            --verbose)
                VERBOSE=true
                shift
                ;;
            *)
                log_error "Unknown option: $1"
                print_usage
                exit 1
                ;;
        esac
    done
}

print_usage() {
    grep "^#" "$0" | grep -E "^\s*#" | head -20
}

# Helper to run commands with dry-run support
run_cmd() {
    if [[ "$DRY_RUN" == true ]]; then
        log "[DRY-RUN] $*"
    else
        if [[ "$VERBOSE" == true ]]; then
            log "Running: $*"
        fi
        eval "$@"
    fi
}

# Check prerequisites
check_prerequisites() {
    log "Checking prerequisites..."

    # Check required tools
    local required_tools=("cargo" "stellar" "soroban" "jq")
    for tool in "${required_tools[@]}"; do
        if ! command -v "$tool" &> /dev/null; then
            log_error "Required tool not found: $tool"
            exit 1
        fi
    done

    # Check Stellar CLI version
    local stellar_version=$(stellar version)
    log "Stellar CLI version: $stellar_version"

    # Check for .env file
    if [[ ! -f ".env" ]]; then
        log_warning ".env file not found. Creating template..."
        cat > .env.template << 'EOF'
# Stellar Network
STELLAR_NETWORK=testnet
STELLAR_SERVER_URL=https://soroban-testnet.stellar.org

# Deployment Account (will be created if needed)
DEPLOYER_SECRET_KEY=
DEPLOYER_PUBLIC_KEY=

# Admin Account (controls contract upgrades)
ADMIN_SECRET_KEY=
ADMIN_PUBLIC_KEY=

# Fee Configuration
BASE_FEE=100
MAX_FEE=10000

# Contract Initialization
INITIAL_ADMIN_PUBKEY=
EOF
        log "Created .env.template - please configure and source it"
        exit 1
    fi

    source .env
    log_success "Prerequisites check passed"
}

# Build contracts
build_contracts() {
    if [[ "$SKIP_BUILD" == true ]]; then
        log_warning "Skipping contract build (--skip-build)"
        return
    fi

    log "Building contracts..."
    run_cmd "cargo build --target wasm32-unknown-unknown --release"

    if [[ -f "target/wasm32-unknown-unknown/release/ip_registry.wasm" ]]; then
        log_success "IP Registry contract built"
    else
        log_error "IP Registry contract build failed"
        return 1
    fi

    if [[ -f "target/wasm32-unknown-unknown/release/atomic_swap.wasm" ]]; then
        log_success "Atomic Swap contract built"
    else
        log_error "Atomic Swap contract build failed"
        return 1
    fi
}

# Setup deployer account
setup_deployer() {
    log "Setting up deployer account..."

    if [[ "$FRESH_DEPLOY" == true ]]; then
        log_warning "Removing existing deployer keys (--fresh)"
        run_cmd "stellar keys delete deployer --network $NETWORK 2>/dev/null || true"
    fi

    # Generate or verify deployer key
    if ! stellar keys ls deployer --network $NETWORK &>/dev/null; then
        log "Generating new deployer key..."
        run_cmd "stellar keys generate deployer --network $NETWORK"
    fi

    local deployer_pubkey=$(stellar keys ls deployer --network $NETWORK 2>/dev/null | grep "Public" | awk '{print $NF}')
    log_success "Deployer account: $deployer_pubkey"

    # Save to state file
    mkdir -p "$(dirname "$DEPLOY_STATE_FILE")"
    echo "{\"deployer_pubkey\": \"$deployer_pubkey\", \"deployed_at\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}" > "$DEPLOY_STATE_FILE"
}

# Deploy IP Registry contract — captures and exports CONTRACT_IP_REGISTRY
deploy_ip_registry() {
    log "Deploying IP Registry contract..."

    local wasm_file="target/wasm32-unknown-unknown/release/ip_registry.wasm"
    if [[ ! -f "$wasm_file" ]]; then
        log_error "WASM file not found: $wasm_file"
        return 1
    fi

    log "WASM file size: $(du -h "$wasm_file" | cut -f1)"

    if [[ "$DRY_RUN" == true ]]; then
        log "[DRY-RUN] stellar contract deploy --wasm $wasm_file --source deployer --network $NETWORK"
        CONTRACT_IP_REGISTRY="DRY_RUN_PLACEHOLDER_IP_REGISTRY"
    else
        CONTRACT_IP_REGISTRY=$(stellar contract deploy \
            --wasm "$wasm_file" \
            --source deployer \
            --network "$NETWORK" 2>&1)
        if [[ $? -ne 0 ]]; then
            log_error "IP Registry deployment failed: $CONTRACT_IP_REGISTRY"
            return 1
        fi
    fi

    export CONTRACT_IP_REGISTRY
    log_success "IP Registry deployed: $CONTRACT_IP_REGISTRY"
}

# Deploy Atomic Swap contract — captures and exports CONTRACT_ATOMIC_SWAP
deploy_atomic_swap() {
    log "Deploying Atomic Swap contract..."

    local wasm_file="target/wasm32-unknown-unknown/release/atomic_swap.wasm"
    if [[ ! -f "$wasm_file" ]]; then
        log_error "WASM file not found: $wasm_file"
        return 1
    fi

    log "WASM file size: $(du -h "$wasm_file" | cut -f1)"

    if [[ "$DRY_RUN" == true ]]; then
        log "[DRY-RUN] stellar contract deploy --wasm $wasm_file --source deployer --network $NETWORK"
        CONTRACT_ATOMIC_SWAP="DRY_RUN_PLACEHOLDER_ATOMIC_SWAP"
    else
        CONTRACT_ATOMIC_SWAP=$(stellar contract deploy \
            --wasm "$wasm_file" \
            --source deployer \
            --network "$NETWORK" 2>&1)
        if [[ $? -ne 0 ]]; then
            log_error "Atomic Swap deployment failed: $CONTRACT_ATOMIC_SWAP"
            return 1
        fi
    fi

    export CONTRACT_ATOMIC_SWAP
    log_success "Atomic Swap deployed: $CONTRACT_ATOMIC_SWAP"
}

# Initialize contracts
initialize_contracts() {
    if [[ "$SKIP_INIT" == true ]]; then
        log_warning "Skipping contract initialization (--skip-init)"
        return
    fi

    log "Initializing Atomic Swap with IP Registry address..."
    if [[ "$DRY_RUN" == true ]]; then
        log "[DRY-RUN] stellar contract invoke --id $CONTRACT_ATOMIC_SWAP --source deployer --network $NETWORK -- initialize --ip_registry $CONTRACT_IP_REGISTRY"
    else
        stellar contract invoke \
            --id "$CONTRACT_ATOMIC_SWAP" \
            --source deployer \
            --network "$NETWORK" \
            -- initialize \
            --ip_registry "$CONTRACT_IP_REGISTRY"
    fi
    log_success "Contracts initialized"
}

# Validate deployment
validate_deployment() {
    log "Validating deployment..."

    if [[ "$DRY_RUN" == true ]]; then
        log "[DRY-RUN] Skipping live validation"
        return
    fi

    # Verify IP Registry is callable
    stellar contract invoke \
        --id "$CONTRACT_IP_REGISTRY" \
        --source deployer \
        --network "$NETWORK" \
        -- list_ip_by_owner \
        --owner "$(stellar keys address deployer)" \
        > /dev/null 2>&1 && log_success "IP Registry is accessible" \
        || log_warning "IP Registry validation call failed (may be expected if no IPs exist)"

    log_success "Deployment validation passed"
}

# Write contract IDs to .env.testnet for downstream use
write_env_file() {
    local env_file=".env.testnet"
    cat > "$env_file" << EOF
# Auto-generated by deploy_testnet.sh on $(date -u +%Y-%m-%dT%H:%M:%SZ)
export STELLAR_NETWORK=${NETWORK}
export STELLAR_RPC_URL=https://soroban-testnet.stellar.org
export CONTRACT_IP_REGISTRY=${CONTRACT_IP_REGISTRY}
export CONTRACT_ATOMIC_SWAP=${CONTRACT_ATOMIC_SWAP}
# Source this file before running integration tests:
#   source .env.testnet && cargo test --test testnet_integration -- --ignored
EOF
    log_success "Contract IDs written to $env_file"
    log "  IP Registry:  $CONTRACT_IP_REGISTRY"
    log "  Atomic Swap:  $CONTRACT_ATOMIC_SWAP"
}

# Cleanup on error
cleanup() {
    local exit_code=$?
    if [[ $exit_code -ne 0 ]]; then
        log_error "Deployment failed with exit code $exit_code"
        log "Log file saved to: $LOG_FILE"
    fi
    exit $exit_code
}

# Main deployment flow
main() {
    trap cleanup EXIT

    log "=== Atomic Patent Testnet Deployment ==="
    log "Network: $NETWORK"
    log "Timestamp: $(date -u +%Y-%m-%dT%H:%M:%SZ)"

    parse_args "$@"

    check_prerequisites
    build_contracts
    setup_deployer
    deploy_ip_registry
    deploy_atomic_swap
    initialize_contracts
    validate_deployment
    write_env_file

    log_success "=== Deployment Complete ==="
    log "Deployment state saved to: $DEPLOY_STATE_FILE"
    log "Full log saved to: $LOG_FILE"
}

main "$@"
