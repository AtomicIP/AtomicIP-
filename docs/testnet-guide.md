# Testnet Deployment Guide

This guide walks through deploying Atomic Patent contracts to the Stellar testnet, testing the complete atomic swap workflow, and validating contract functionality.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Environment Setup](#environment-setup)
3. [Contract Deployment](#contract-deployment)
4. [Contract Initialization](#contract-initialization)
5. [Testing Atomic Swap Flow](#testing-atomic-swap-flow)
6. [Fee Calculation](#fee-calculation)
7. [Token Transfers](#token-transfers)
8. [Troubleshooting](#troubleshooting)

## Prerequisites

### Required Software

- Rust 1.70+ (with `wasm32-unknown-unknown` target)
- Stellar CLI 21.0+
- Soroban CLI 20.0+
- jq (for JSON processing)

### Testnet Accounts

You'll need two Stellar testnet accounts:
- **Deployer Account**: Used to deploy contracts
- **Admin Account**: Controls contract upgrades and configuration

Create accounts using the Stellar Laboratory: https://lab.stellar.org

Fund accounts with testnet Lumens from: https://friendbot.stellar.org/

### Environment Variables

Create a `.env` file in the repository root:

```bash
# Stellar Configuration
export STELLAR_NETWORK=testnet
export STELLAR_SERVER_URL=https://soroban-testnet.stellar.org
export STELLAR_NETWORK_PASSPHRASE="Test SDF Network ; September 2015"

# Account Keys
export DEPLOYER_SECRET_KEY="SXXXXXXXXX..."  # Deployment account
export ADMIN_SECRET_KEY="SXXXXXXXXX..."     # Admin account

# Deployed Contract IDs (populated after deployment)
export IP_REGISTRY_CONTRACT_ID=""
export ATOMIC_SWAP_CONTRACT_ID=""

# RPC Configuration
export SOROBAN_RPC_HOST=https://soroban-testnet.stellar.org
export SOROBAN_RPC_PORT=443
export SOROBAN_NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
```

## Environment Setup

### 1. Install Dependencies

```bash
# Install Stellar CLI
curl -s https://install.stellar.org | bash

# Install Soroban CLI
cargo install soroban-cli --version 20.0

# Verify installations
stellar version
soroban --version
```

### 2. Fund Test Accounts

```bash
# Get account IDs from CLI
stellar keys ls deployer --network testnet
stellar keys ls admin --network testnet

# Fund via Friendbot
curl "https://friendbot.stellar.org/?addr=$(stellar keys ls deployer --network testnet | grep Public | awk '{print $NF}')"
curl "https://friendbot.stellar.org/?addr=$(stellar keys ls admin --network testnet | grep Public | awk '{print $NF}')"

# Verify funding
stellar account info deployer --network testnet
```

### 3. Verify Network Connectivity

```bash
# Test RPC connection
curl -X POST https://soroban-testnet.stellar.org/soroban/rpc \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getLatestLedger",
    "params": []
  }'

# Output should include ledger_version, network passphrase, protocol_version
```

## Contract Deployment

### 1. Build Contracts

```bash
# Build both contracts for wasm32 target
cargo build --target wasm32-unknown-unknown --release

# Verify WASM artifacts
ls -lh target/wasm32-unknown-unknown/release/*.wasm
```

### 2. Deploy IP Registry Contract

```bash
# Deploy
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/ip_registry.wasm \
  --source deployer \
  --network testnet

# Output will show the contract ID
# Save it:
export IP_REGISTRY_CONTRACT_ID="CXXXXXXXXX..."
```

### 3. Deploy Atomic Swap Contract

```bash
# Deploy
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/atomic_swap.wasm \
  --source deployer \
  --network testnet

# Save contract ID
export ATOMIC_SWAP_CONTRACT_ID="CXXXXXXXXX..."
```

### 4. Verify Deployments

```bash
# Get contract details
soroban contract info $IP_REGISTRY_CONTRACT_ID --network testnet
soroban contract info $ATOMIC_SWAP_CONTRACT_ID --network testnet

# Verify contract storage is initialized
soroban contract read \
  --id $IP_REGISTRY_CONTRACT_ID \
  --key-xdr $(soroban contract build-xdr --contract key next_id) \
  --network testnet
```

## Contract Initialization

### 1. Initialize Atomic Swap Contract

The Atomic Swap contract requires initialization with the IP Registry contract ID:

```bash
# Create initialization transaction
soroban contract invoke \
  --id $ATOMIC_SWAP_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- \
  initialize \
  --registry $IP_REGISTRY_CONTRACT_ID
```

### 2. Verify Initialization

```bash
# Check if registry is set
soroban contract invoke \
  --id $ATOMIC_SWAP_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- \
  get_registry

# Should output the IP Registry contract ID
```

## Testing Atomic Swap Flow

### 1. Create Test Accounts

```bash
# Generate test accounts (or use existing ones)
stellar keys generate seller --network testnet
stellar keys generate buyer --network testnet

# Get public keys
SELLER_PUB=$(stellar keys ls seller --network testnet | grep Public | awk '{print $NF}')
BUYER_PUB=$(stellar keys ls buyer --network testnet | grep Public | awk '{print $NF}')

# Fund them
curl "https://friendbot.stellar.org/?addr=$SELLER_PUB"
curl "https://friendbot.stellar.org/?addr=$BUYER_PUB"
```

### 2. Seller: Create IP Commitment

```bash
# Generate secret and blinding factor
SECRET=$(openssl rand -hex 32)
BLINDING=$(openssl rand -hex 32)

# Compute commitment hash (sha256(secret || blinding))
PREIMAGE=$(echo -n "${SECRET}${BLINDING}" | xxd -r -p)
COMMITMENT=$(echo -n "$PREIMAGE" | sha256sum | cut -d' ' -f1)

echo "Secret: $SECRET"
echo "Blinding: $BLINDING"
echo "Commitment: $COMMITMENT"

# Call commit_ip
soroban contract invoke \
  --id $IP_REGISTRY_CONTRACT_ID \
  --source seller \
  --network testnet \
  -- \
  commit_ip \
  --owner $SELLER_PUB \
  --commitment_hash $COMMITMENT

# Save returned IP ID
export IP_ID=<returned_id>
```

### 3. Buyer: Create/Fund Test Token

```bash
# Create a test token (if not using native Lumens)
TOKEN_ID=$(soroban contract invoke \
  --id $ATOMIC_SWAP_CONTRACT_ID \
  --source buyer \
  --network testnet \
  -- \
  create_test_token \
  --decimals 7)

echo "Test token: $TOKEN_ID"

# Mint tokens to buyer
soroban contract invoke \
  --id $TOKEN_ID \
  --source buyer \
  --network testnet \
  -- \
  mint \
  --to $BUYER_PUB \
  --amount 1000000000
```

### 4. Initiate Swap

Seller initiates the atomic swap:

```bash
# Seller: Initiate swap
# Requires: buyer will pay 500 units to receive decryption key
SWAP_ID=$(soroban contract invoke \
  --id $ATOMIC_SWAP_CONTRACT_ID \
  --source seller \
  --network testnet \
  -- \
  initiate_swap \
  --token $TOKEN_ID \
  --ip_id $IP_ID \
  --seller $SELLER_PUB \
  --amount 500 \
  --buyer $BUYER_PUB)

echo "Swap ID: $SWAP_ID"
```

### 5. Accept Swap

Buyer accepts the swap:

```bash
# Buyer: Accept the swap
# This locks payment in escrow
soroban contract invoke \
  --id $ATOMIC_SWAP_CONTRACT_ID \
  --source buyer \
  --network testnet \
  -- \
  accept_swap \
  --swap_id $SWAP_ID

# Verify swap is accepted
soroban contract invoke \
  --id $ATOMIC_SWAP_CONTRACT_ID \
  --source buyer \
  --network testnet \
  -- \
  get_swap \
  --swap_id $SWAP_ID
```

### 6. Reveal Decryption Key

Seller reveals the decryption key to unlock payment transfer:

```bash
# Seller: Reveal the decryption key
# Using the secret from step 2
soroban contract invoke \
  --id $ATOMIC_SWAP_CONTRACT_ID \
  --source seller \
  --network testnet \
  -- \
  reveal_key \
  --swap_id $SWAP_ID \
  --secret $SECRET

# Verify payment was transferred
soroban contract invoke \
  --id $TOKEN_ID \
  --source buyer \
  --network testnet \
  -- \
  balance \
  --account $SELLER_PUB
```

## Fee Calculation

### 1. Understand Fee Structure

Stellar soroban fees consist of:
- **Base Fee**: Network minimum (100 stroops = 0.00001 XLM)
- **Resource Fee**: Based on CPU, memory, ledger operations
- **Ledger Fee**: For persistent data writes

### 2. Estimate Fees

```bash
# Estimate fee for commit_ip
soroban contract invoke \
  --id $IP_REGISTRY_CONTRACT_ID \
  --source seller \
  --network testnet \
  --estimate-only \
  -- \
  commit_ip \
  --owner $SELLER_PUB \
  --commitment_hash $COMMITMENT

# Estimate fee for atomic swap
soroban contract invoke \
  --id $ATOMIC_SWAP_CONTRACT_ID \
  --source buyer \
  --network testnet \
  --estimate-only \
  -- \
  accept_swap \
  --swap_id $SWAP_ID
```

### 3. Monitor Actual Fees

```bash
# Check transaction fees in ledger
stellar transaction info <tx_hash> --network testnet

# Analyze fee patterns across multiple transactions
# Use for cost estimation and optimization
```

## Token Transfers

### 1. Verify Escrow Mechanism

During swap acceptance, the buyer's payment is held in contract escrow:

```bash
# Check escrow balance
soroban contract invoke \
  --id $ATOMIC_SWAP_CONTRACT_ID \
  --source buyer \
  --network testnet \
  -- \
  get_swap \
  --swap_id $SWAP_ID
  
# Should show payment in escrow state
```

### 2. Test Payment Release

When decryption key is revealed, payment transfers to seller:

```bash
# Before reveal - check seller balance
SELLER_BALANCE_BEFORE=$(soroban contract invoke \
  --id $TOKEN_ID \
  --source seller \
  --network testnet \
  -- \
  balance \
  --account $SELLER_PUB)

# Reveal key (as done above)

# After reveal - verify transfer
SELLER_BALANCE_AFTER=$(soroban contract invoke \
  --id $TOKEN_ID \
  --source seller \
  --network testnet \
  -- \
  balance \
  --account $SELLER_PUB)

echo "Before: $SELLER_BALANCE_BEFORE"
echo "After: $SELLER_BALANCE_AFTER"
```

## Troubleshooting

### Contract Not Found

```
Error: Contract not found at CXXXXXXXXX
```

**Solutions:**
- Verify contract ID is correct: `echo $IP_REGISTRY_CONTRACT_ID`
- Check contract was deployed: `soroban contract info $IP_REGISTRY_CONTRACT_ID`
- Ensure you're using correct network: `--network testnet`

### Authorization Failed

```
Error: Unauthorized
```

**Solutions:**
- Verify account is funded: `stellar account info <account> --network testnet`
- Check account has valid key: `stellar keys ls`
- Confirm auth mock is enabled: `env.mock_all_auths()`

### Transaction Failed

```
Error: Transaction failed
```

**Solutions:**
- Check transaction details: `stellar transaction info <tx_hash> --network testnet`
- Review error code: consult contract error enums
- Increase fee if due to network congestion

### Insufficient Balance

```
Error: Insufficient balance
```

**Solutions:**
- Fund account: `curl "https://friendbot.stellar.org/?addr=$ACCOUNT_PUB"`
- Check balance: `stellar account info <account>`
- Calculate required amount: base_fee + resource_fee + ledger_fee

### Network Connection Issues

```
Error: Failed to connect to soroban-testnet.stellar.org
```

**Solutions:**
- Verify connection: `ping soroban-testnet.stellar.org`
- Check network settings: `soroban config network ls`
- Verify RPC endpoint: check `SOROBAN_RPC_HOST`

## Next Steps

After successful testnet deployment and testing:

1. **Document Results**: Record gas usage, fees, performance metrics
2. **Review Logs**: Check for warnings or optimization opportunities
3. **Security Review**: Validate against security checklist
4. **Prepare for Mainnet**: Follow [deployment-guide.md](./deployment-guide.md) for production
5. **Monitor**: Set up monitoring and alerting for production contracts

## Additional Resources

- [Stellar Soroban Docs](https://developers.stellar.org/docs/)
- [Soroban CLI Reference](https://github.com/stellar/rs-soroban-sdk)
- [Testnet Status](https://status.stellar.org/)
- [Stellar Discord](https://discord.gg/hardcoredev)
