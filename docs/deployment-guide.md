# Mainnet Deployment Guide

This guide covers the production deployment of Atomic Patent to the Stellar mainnet. **Extreme care must be taken** to ensure security, irreversibility, and proper configuration.

**⚠️ WARNING**: Mainnet deployments are permanent and irreversible. Smart contract bugs can result in total loss of funds. Follow all procedures carefully and conduct thorough audits before deployment.

## Table of Contents

1. [Pre-Deployment Checklist](#pre-deployment-checklist)
2. [Environment Setup](#environment-setup)
3. [Key Management](#key-management)
4. [Contract Deployment](#contract-deployment)
5. [Contract Initialization](#contract-initialization)
6. [Post-Deployment Verification](#post-deployment-verification)
7. [Monitoring & Operations](#monitoring--operations)
8. [Emergency Procedures](#emergency-procedures)
9. [Rollback Procedures](#rollback-procedures)

## Pre-Deployment Checklist

### Security Review

- [ ] All contracts have been through formal security audit
- [ ] All vulnerabilities from audit have been addressed
- [ ] Code has been reviewed for reentrancy issues
- [ ] Integer overflow/underflow checks are in place
- [ ] Storage expiry (TTL) is properly managed
- [ ] Auth model has been validated
- [ ] Signature verification is correct
- [ ] Edge cases have been tested with fuzz tests
- [ ] Testnet deployment and testing completed successfully
- [ ] Performance testing completed and acceptable

### Operational Review

- [ ] Admin accounts are secure and backed up
- [ ] Key rotation procedures documented
- [ ] Rate limiting is configured
- [ ] Audit logging is enabled
- [ ] Monitoring is configured
- [ ] Rollback procedure is tested
- [ ] Emergency contacts are established
- [ ] Support playbooks are prepared

### Financial Review

- [ ] Budget for mainnet gas fees estimated
- [ ] Fee structure verified with Stellar team
- [ ] Cost projections reviewed
- [ ] Funding accounts prepared with sufficient reserves

## Environment Setup

### 1. Production Account Setup

```bash
# Create production keys (do NOT use test keys)
stellar keys generate mainnet-deployer --network public
stellar keys generate mainnet-admin --network public

# Display keys (SAFELY - never log these)
stellar keys ls mainnet-deployer --network public
stellar keys ls mainnet-admin --network public
```

### 2. Secure Key Storage

**CRITICAL**: Keys must be stored securely:

```bash
# Option 1: Hardware Wallet (RECOMMENDED for mainnet)
# Store keys on Ledger, Trezor, or similar hardware wallet

# Option 2: Encrypted File (if hardware unavailable)
# Encrypt keys with a strong password:
gpg --symmetric --cipher-algo AES256 ~/.stellar/keys/mainnet-deployer

# Option 3: Key Management Service (for enterprise deployments)
# Use AWS KMS, Google Cloud KMS, or similar service
```

### 3. Environment Configuration

Create `.env.mainnet` with production settings:

```bash
# Stellar Network Configuration
export STELLAR_NETWORK=public
export STELLAR_SERVER_URL=https://horizon.stellar.org
export STELLAR_NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
export SOROBAN_RPC_HOST=https://soroban.stellar.org
export SOROBAN_RPC_PORT=443

# Account Configuration
export DEPLOYER_ACCOUNT=$(stellar keys ls mainnet-deployer --network public | grep Public | awk '{print $NF}')
export ADMIN_ACCOUNT=$(stellar keys ls mainnet-admin --network public | grep Public | awk '{print $NF}')

# Operational Settings
export BASE_FEE=100          # stroops (0.00001 XLM)
export MAX_FEE=100000        # stroops (0.01 XLM maximum)
export LOG_LEVEL=warn
export ENABLE_AUDIT_LOG=true

# Upgraded Contract IDs (populated during deployment)
export IP_REGISTRY_CONTRACT_ID=""
export ATOMIC_SWAP_CONTRACT_ID=""

# Deployment Metadata
export DEPLOYMENT_DATE=$(date -u +%Y-%m-%d)
export DEPLOYMENT_REVISION=$(git rev-parse HEAD)
```

Load configuration:

```bash
set -a
source .env.mainnet
set +a
```

### 4. Fund Production Accounts

Ensure accounts have sufficient XLM for deployment and operations:

```bash
# Check account balances
stellar account info $DEPLOYER_ACCOUNT --network public
stellar account info $ADMIN_ACCOUNT --network public

# Recommended minimum balance:
# - Deployer: 10 XLM (for deployment gas)
# - Admin: 5 XLM (for admin operations)
```

## Key Management

### 1. Key Rotation Strategy

Document and implement key rotation:

```bash
# Generate new admin key
stellar keys generate mainnet-admin-new --network public

# Plan for graceful key rotation:
# 1. Update admin key in contract
# 2. Wait for confirmation period
# 3. Archive old key
# 4. Destroy/secure old key
```

### 2. Multi-Signature Administration

For high-security deployments, use multi-sig:

```bash
# Create multi-sig account (example with 2-of-3)
# This requires coordination with Stellar CLI or custom tooling

# Step 1: Create 3 signer accounts
# Step 2: Configure account with 2-of-3 signing requirement
# Step 3: Document all signers and their responsibilities
```

### 3. Key Backup

Encrypted backups of all keys must be maintained:

```bash
# Create secure backup
gpg --symmetric --cipher-algo AES256 ~/.stellar/keys/mainnet-admin > mainnet-admin.key.gpg

# Backup to secure location:
# - Cloud encrypted storage (OneDrive, Dropbox with encryption)
# - Hardware storage (encrypted USB drive)
# - Paper backup (written out, stored in safe)

# Test recovery from backup
mkdir temp-restore
gpg -d mainnet-admin.key.gpg > temp-restore/key
cat temp-restore/key | stellar keys import mainnet-admin-test
```

## Contract Deployment

### 1. Final Pre-Deployment Verification

```bash
# Verify contract code hasn't changed since audit
AUDIT_HASH="abc123..."  # From security audit report
CURRENT_HASH=$(sha256sum target/wasm32-unknown-unknown/release/ip_registry.wasm | cut -d' ' -f1)

if [ "$AUDIT_HASH" != "$CURRENT_HASH" ]; then
    echo "ERROR: Contract code has changed since audit!"
    exit 1
fi

echo "✓ Contract code verified"
```

### 2. Deploy IP Registry

```bash
# Deploy IP Registry contract
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/ip_registry.wasm \
  --source mainnet-deployer \
  --network public \
  --description "Atomic Patent IP Registry - Mainnet Production"

# Save returned contract ID
read -p "Enter deployed IP Registry contract ID: " IP_REGISTRY_CONTRACT_ID
export IP_REGISTRY_CONTRACT_ID

# Verify deployment
soroban contract info $IP_REGISTRY_CONTRACT_ID --network public
echo "IP_REGISTRY_CONTRACT_ID=$IP_REGISTRY_CONTRACT_ID" >> .env.mainnet
```

### 3. Deploy Atomic Swap

```bash
# Deploy Atomic Swap contract
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/atomic_swap.wasm \
  --source mainnet-deployer \
  --network public \
  --description "Atomic Patent Atomic Swap - Mainnet Production"

# Save returned contract ID
read -p "Enter deployed Atomic Swap contract ID: " ATOMIC_SWAP_CONTRACT_ID
export ATOMIC_SWAP_CONTRACT_ID

# Verify deployment
soroban contract info $ATOMIC_SWAP_CONTRACT_ID --network public
echo "ATOMIC_SWAP_CONTRACT_ID=$ATOMIC_SWAP_CONTRACT_ID" >> .env.mainnet
```

### 4. Record Deployment

Create deployment record for audit trail:

```bash
cat > deployment-record-$(date +%Y%m%d).json << EOF
{
  "deployment_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "network": "public",
  "deployer": "$DEPLOYER_ACCOUNT",
  "ip_registry_contract": "$IP_REGISTRY_CONTRACT_ID",
  "atomic_swap_contract": "$ATOMIC_SWAP_CONTRACT_ID",
  "code_revision": "$(git rev-parse HEAD)",
  "code_hash_ip_registry": "$(sha256sum target/wasm32-unknown-unknown/release/ip_registry.wasm | cut -d' ' -f1)",
  "code_hash_atomic_swap": "$(sha256sum target/wasm32-unknown-unknown/release/atomic_swap.wasm | cut -d' ' -f1)"
}
EOF

# Store securely and back up
echo "Deployment record saved"
```

## Contract Initialization

### 1. Initialize Atomic Swap Contract

```bash
# Link Atomic Swap to IP Registry
soroban contract invoke \
  --id $ATOMIC_SWAP_CONTRACT_ID \
  --source mainnet-deployer \
  --network public \
  -- \
  initialize \
  --registry $IP_REGISTRY_CONTRACT_ID

echo "✓ Atomic Swap initialized with IP Registry"
```

### 2. Set Admin Account

```bash
# Set the admin account (for future upgrades)
soroban contract invoke \
  --id $IP_REGISTRY_CONTRACT_ID \
  --source mainnet-deployer \
  --network public \
  -- \
  set_admin \
  --new_admin $ADMIN_ACCOUNT

echo "✓ Admin account configured"
```

### 3. Configure Protocol Parameters

```bash
# Set protocol configuration if contract supports it
# This might include fee structures, rate limits, etc.

soroban contract invoke \
  --id $ATOMIC_SWAP_CONTRACT_ID \
  --source mainnet-admin \
  --network public \
  -- \
  configure_protocol \
  --max_swap_amount 1000000000 \
  --rate_limit_per_minute 100 \
  --enable_audit_log true

echo "✓ Protocol parameters configured"
```

## Post-Deployment Verification

### 1. Contract Availability Check

```bash
# Verify contracts are callable
soroban contract invoke \
  --id $IP_REGISTRY_CONTRACT_ID \
  --source mainnet-admin \
  --network public \
  -- \
  get_next_id

echo "✓ IP Registry contract is responsive"

# Verify Atomic Swap registry link
soroban contract invoke \
  --id $ATOMIC_SWAP_CONTRACT_ID \
  --source mainnet-admin \
  --network public \
  -- \
  get_registry

echo "✓ Atomic Swap contract is responsive"
```

### 2. Verify Storage State

```bash
# Verify no unexpected data exists
soroban contract invoke \
  --id $IP_REGISTRY_CONTRACT_ID \
  --source mainnet-admin \
  --network public \
  -- \
  get_ips_by_owner \
  --owner $ADMIN_ACCOUNT

# Should return empty vector for fresh deployment
echo "✓ Storage state verified"
```

### 3. Test Transaction Flow (Optional - Testnet Replica)

For maximum confidence, test on a testnet replica:

```bash
# Deploy same code to testnet
# Run full atomic swap flow
# Verify behavior matches expectations
# Only then consider mainnet "verified"
```

## Monitoring & Operations

### 1. Setup Monitoring

Configure monitoring for contract health:

```bash
# Monitor contract storage usage
# Monitor transaction success rates
# Monitor average fees
# Set up alerts for:
#   - Unusual activity patterns
#   - High fee transactions
#   - Transaction failures
#   - Storage capacity issues
```

### 2. Audit Logging

Implement comprehensive logging:

```bash
# Enable audit logging in contract configuration
# Log all state-changing operations
# Maintain audit trail for compliance
# Archive logs regularly
# Test log recovery procedures
```

### 3. Performance Baseline

Establish baseline metrics:

```bash
# Average transaction fee: X stroops
# Average confirmation time: Y seconds
# Storage growth rate: Z bytes/day
# Peak TPS capacity: N transactions/second
```

### 4. Emergency Contacts

Maintain emergency contact information:

```
Security Team Lead: [Name] [Email] [Phone]
Ops Team Lead: [Name] [Email] [Phone]
Product Lead: [Name] [Email] [Phone]
Stellar Support: https://stellar.org/developers/support
```

## Emergency Procedures

### Critical Vulnerability Discovery

If a critical vulnerability is discovered:

1. **Immediate**: Halt all new transactions (via admin pausable mechanism if available)
2. **First Hour**: Assemble security team
3. **First 6 Hours**: Assess impact and develop fix
4. **First Day**: Deploy patched contract (if possible) or communicate issue
5. **Ongoing**: Monitor for exploitation, maintain audit trail

### Disaster Recovery

In case of complete contract failure:

```bash
# Step 1: Determine failure cause
# Step 2: Assess financial impact
# Step 3: Prepare communication to users
# Step 4: Prepare rollback strategy
# Step 5: Execute rollback (see below)
```

## Rollback Procedures

### Prepare Rollback

Before fully committing to mainnet, maintain rollback capability:

```bash
# Create snapshot of contract state before any critical operations
# Document exact steps to restore from snapshot
# Test rollback procedure on testnet
# Maintain secure backup of pre-deployment state
```

### Execute Rollback (if necessary)

```bash
# WARNING: Rollback should only be attempted with professional guidance

# Step 1: Stop accepting new transactions
# Step 2: Backup current state for forensics
# Step 3: Deploy previous working version
# Step 4: Restore known-good state
# Step 5: Verify restoration worked
# Step 6: Communicate to users
```

## Maintenance Schedule

### Daily (Automated)

- [ ] Check contract health
- [ ] Monitor error rates
- [ ] Review emergency alerts

### Weekly (Manual)

- [ ] Review transaction logs
- [ ] Check storage growth
- [ ] Validate monitoring systems

### Monthly (Planned)

- [ ] Review security logs
- [ ] Update baseline metrics
- [ ] Plan any necessary maintenance

### Quarterly (Strategic)

- [ ] Security audit review
- [ ] Performance optimization analysis
- [ ] Upgrade planning

## Next Steps After Deployment

1. **Announce Production**: Notify users through official channels
2. **Monitor Closely**: First week requires active monitoring
3. **Document Operations**: Create runbooks for common operations
4. **Plan Upgrades**: Document procedures for contract upgrades
5. **Long-term Strategy**: Plan for contract versioning and migration path

## Additional Resources

- [Stellar Production Readiness Checklist](https://developers.stellar.org/docs)
- [Soroban Security Best Practices](https://developers.stellar.org/docs/learn/security)
- [Stellar Help Center](https://stellar.org/developers/support)
- [Network Status](https://status.stellar.org/)

## Documentation

For detailed information, see:
- [Testnet Deployment Guide](./testnet-guide.md)
- [Security Audit Checklist](./security-audit-checklist.md)
- [Threat Model](./threat-model.md)
- [API Reference](./api-reference.md)
