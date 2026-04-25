# Contract State Backup and Recovery Guide

## Overview

This guide provides procedures for backing up and recovering contract state for the AtomicIP platform. Regular backups ensure data integrity and enable disaster recovery.

## Backup Strategy

### What to Backup

1. Contract State Data
   - IP Registry records
   - Atomic Swap records
   - User mappings and indices
   - Configuration data

2. Transaction History
   - All contract invocations
   - Event logs
   - State transitions

3. Configuration
   - Contract addresses
   - Admin keys
   - Network configuration

### Backup Frequency

- Full state backup: Daily at 00:00 UTC
- Incremental backup: Every 6 hours
- Transaction logs: Real-time streaming
- Configuration: On every change

## Backup Procedures

### 1. Full State Backup

```bash
#!/bin/bash
# backup-contract-state.sh

BACKUP_DIR="/var/backups/atomicip"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
NETWORK="mainnet"  # or testnet

# Create backup directory
mkdir -p "$BACKUP_DIR/$TIMESTAMP"

# Export IP Registry state
stellar-cli contract invoke \
  --id $IP_REGISTRY_CONTRACT_ID \
  --network $NETWORK \
  -- export_state > "$BACKUP_DIR/$TIMESTAMP/ip_registry_state.json"

# Export Atomic Swap state
stellar-cli contract invoke \
  --id $ATOMIC_SWAP_CONTRACT_ID \
  --network $NETWORK \
  -- export_state > "$BACKUP_DIR/$TIMESTAMP/atomic_swap_state.json"

# Backup contract metadata
echo "{
  \"timestamp\": \"$TIMESTAMP\",
  \"network\": \"$NETWORK\",
  \"ip_registry_contract\": \"$IP_REGISTRY_CONTRACT_ID\",
  \"atomic_swap_contract\": \"$ATOMIC_SWAP_CONTRACT_ID\",
  \"ledger_sequence\": $(stellar-cli network status --network $NETWORK | jq .ledger)
}" > "$BACKUP_DIR/$TIMESTAMP/metadata.json"

# Compress backup
tar -czf "$BACKUP_DIR/backup_$TIMESTAMP.tar.gz" -C "$BACKUP_DIR" "$TIMESTAMP"
rm -rf "$BACKUP_DIR/$TIMESTAMP"

# Upload to remote storage (S3, GCS, etc.)
aws s3 cp "$BACKUP_DIR/backup_$TIMESTAMP.tar.gz" \
  "s3://atomicip-backups/$NETWORK/" \
  --storage-class STANDARD_IA

echo "Backup completed: backup_$TIMESTAMP.tar.gz"
```

### 2. Incremental Backup

```bash
#!/bin/bash
# incremental-backup.sh

LAST_BACKUP_LEDGER=$(cat /var/lib/atomicip/last_backup_ledger)
CURRENT_LEDGER=$(stellar-cli network status --network mainnet | jq .ledger)

# Export events since last backup
stellar-cli events \
  --id $IP_REGISTRY_CONTRACT_ID \
  --start-ledger $LAST_BACKUP_LEDGER \
  --end-ledger $CURRENT_LEDGER \
  > "/var/backups/atomicip/incremental_$(date +%Y%m%d_%H%M%S).json"

echo $CURRENT_LEDGER > /var/lib/atomicip/last_backup_ledger
```

### 3. Transaction Log Streaming

```bash
#!/bin/bash
# stream-transaction-logs.sh

# Stream contract events to log file
stellar-cli events \
  --id $IP_REGISTRY_CONTRACT_ID \
  --follow \
  | tee -a /var/log/atomicip/ip_registry_events.log

stellar-cli events \
  --id $ATOMIC_SWAP_CONTRACT_ID \
  --follow \
  | tee -a /var/log/atomicip/atomic_swap_events.log
```

## Recovery Procedures

### Scenario 1: Contract State Corruption

If contract state becomes corrupted but the blockchain is intact:

1. Identify the last known good ledger
2. Replay transactions from that ledger
3. Verify state consistency

```bash
#!/bin/bash
# recover-from-ledger.sh

RECOVERY_LEDGER=$1
CURRENT_LEDGER=$(stellar-cli network status --network mainnet | jq .ledger)

# Extract all transactions affecting our contracts
stellar-cli events \
  --id $IP_REGISTRY_CONTRACT_ID \
  --start-ledger $RECOVERY_LEDGER \
  --end-ledger $CURRENT_LEDGER \
  > recovery_events.json

# Verify state consistency
./verify-state.sh recovery_events.json
```

### Scenario 2: Complete Contract Loss

If contracts need to be redeployed:

1. Deploy new contract instances
2. Restore state from backup
3. Update contract addresses in configuration
4. Verify all data integrity

```bash
#!/bin/bash
# redeploy-and-restore.sh

# Deploy new contracts
NEW_IP_REGISTRY=$(stellar-cli contract deploy \
  --wasm ip_registry.wasm \
  --network mainnet)

NEW_ATOMIC_SWAP=$(stellar-cli contract deploy \
  --wasm atomic_swap.wasm \
  --network mainnet)

# Restore state from latest backup
LATEST_BACKUP=$(ls -t /var/backups/atomicip/backup_*.tar.gz | head -1)
tar -xzf $LATEST_BACKUP -C /tmp/

# Import state
stellar-cli contract invoke \
  --id $NEW_IP_REGISTRY \
  --network mainnet \
  -- import_state --data "$(cat /tmp/*/ip_registry_state.json)"

stellar-cli contract invoke \
  --id $NEW_ATOMIC_SWAP \
  --network mainnet \
  -- import_state --data "$(cat /tmp/*/atomic_swap_state.json)"

# Update configuration
echo "IP_REGISTRY_CONTRACT_ID=$NEW_IP_REGISTRY" >> .env
echo "ATOMIC_SWAP_CONTRACT_ID=$NEW_ATOMIC_SWAP" >> .env
```

### Scenario 3: Data Center Failure

If primary infrastructure is unavailable:

1. Activate disaster recovery site
2. Restore from remote backups
3. Update DNS/load balancer
4. Verify service availability

```bash
#!/bin/bash
# activate-dr-site.sh

# Download latest backup from remote storage
aws s3 cp \
  "s3://atomicip-backups/mainnet/$(aws s3 ls s3://atomicip-backups/mainnet/ | sort | tail -1 | awk '{print $4}')" \
  /var/backups/atomicip/

# Extract and restore
BACKUP_FILE=$(ls -t /var/backups/atomicip/backup_*.tar.gz | head -1)
tar -xzf $BACKUP_FILE -C /tmp/

# Start services with restored configuration
./redeploy-and-restore.sh

# Update DNS to point to DR site
# (Manual step or automated via Route53/CloudFlare)
```

## Verification Procedures

### State Integrity Check

```bash
#!/bin/bash
# verify-state.sh

# Check IP Registry consistency
stellar-cli contract invoke \
  --id $IP_REGISTRY_CONTRACT_ID \
  --network mainnet \
  -- verify_integrity

# Check Atomic Swap consistency
stellar-cli contract invoke \
  --id $ATOMIC_SWAP_CONTRACT_ID \
  --network mainnet \
  -- verify_integrity

# Compare with backup
diff <(jq -S . current_state.json) <(jq -S . backup_state.json)
```

### Data Consistency Checks

1. Verify all IP records are accessible
2. Verify all swap records match expected states
3. Verify user indices are complete
4. Verify no orphaned records exist

```bash
#!/bin/bash
# consistency-check.sh

# Count records
IP_COUNT=$(stellar-cli contract invoke --id $IP_REGISTRY_CONTRACT_ID -- get_ip_count)
SWAP_COUNT=$(stellar-cli contract invoke --id $ATOMIC_SWAP_CONTRACT_ID -- get_swap_count)

echo "IP Records: $IP_COUNT"
echo "Swap Records: $SWAP_COUNT"

# Verify indices
./verify-indices.sh
```

## Backup Retention Policy

- Daily backups: Retain for 30 days
- Weekly backups: Retain for 90 days
- Monthly backups: Retain for 1 year
- Yearly backups: Retain indefinitely

## Automated Backup Schedule

Add to crontab:

```cron
# Full backup daily at midnight
0 0 * * * /opt/atomicip/scripts/backup-contract-state.sh

# Incremental backup every 6 hours
0 */6 * * * /opt/atomicip/scripts/incremental-backup.sh

# Cleanup old backups weekly
0 2 * * 0 /opt/atomicip/scripts/cleanup-old-backups.sh
```

## Monitoring and Alerts

- Alert if backup fails
- Alert if backup size deviates significantly
- Alert if recovery test fails
- Monitor backup storage capacity

## Testing Recovery Procedures

Perform recovery drills quarterly:

1. Restore to test environment
2. Verify all data integrity
3. Test application functionality
4. Document any issues
5. Update procedures as needed

## Security Considerations

- Encrypt backups at rest and in transit
- Restrict access to backup storage
- Audit backup access logs
- Test backup integrity regularly
- Store encryption keys separately from backups

## Contact Information

- Backup System Admin: [contact]
- On-call Engineer: [contact]
- Security Team: [contact]
