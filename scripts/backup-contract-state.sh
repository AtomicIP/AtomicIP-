#!/bin/bash
# backup-contract-state.sh
# Full contract state backup script

set -e

BACKUP_DIR="${BACKUP_DIR:-/var/backups/atomicip}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
NETWORK="${NETWORK:-mainnet}"
IP_REGISTRY_CONTRACT_ID="${IP_REGISTRY_CONTRACT_ID}"
ATOMIC_SWAP_CONTRACT_ID="${ATOMIC_SWAP_CONTRACT_ID}"

# Validate environment variables
if [ -z "$IP_REGISTRY_CONTRACT_ID" ] || [ -z "$ATOMIC_SWAP_CONTRACT_ID" ]; then
    echo "Error: Contract IDs not set"
    exit 1
fi

# Create backup directory
mkdir -p "$BACKUP_DIR/$TIMESTAMP"

echo "Starting backup at $TIMESTAMP"

# Export IP Registry state
echo "Backing up IP Registry..."
stellar-cli contract invoke \
  --id "$IP_REGISTRY_CONTRACT_ID" \
  --network "$NETWORK" \
  -- list_all_ips > "$BACKUP_DIR/$TIMESTAMP/ip_registry_state.json" || true

# Export Atomic Swap state
echo "Backing up Atomic Swap..."
stellar-cli contract invoke \
  --id "$ATOMIC_SWAP_CONTRACT_ID" \
  --network "$NETWORK" \
  -- list_all_swaps > "$BACKUP_DIR/$TIMESTAMP/atomic_swap_state.json" || true

# Get current ledger
CURRENT_LEDGER=$(stellar-cli network status --network "$NETWORK" 2>/dev/null | jq -r '.ledger // 0')

# Backup contract metadata
echo "Creating metadata..."
cat > "$BACKUP_DIR/$TIMESTAMP/metadata.json" <<EOF
{
  "timestamp": "$TIMESTAMP",
  "network": "$NETWORK",
  "ip_registry_contract": "$IP_REGISTRY_CONTRACT_ID",
  "atomic_swap_contract": "$ATOMIC_SWAP_CONTRACT_ID",
  "ledger_sequence": $CURRENT_LEDGER
}
EOF

# Compress backup
echo "Compressing backup..."
tar -czf "$BACKUP_DIR/backup_$TIMESTAMP.tar.gz" -C "$BACKUP_DIR" "$TIMESTAMP"
rm -rf "$BACKUP_DIR/$TIMESTAMP"

# Upload to remote storage if configured
if [ -n "$BACKUP_S3_BUCKET" ]; then
    echo "Uploading to S3..."
    aws s3 cp "$BACKUP_DIR/backup_$TIMESTAMP.tar.gz" \
      "s3://$BACKUP_S3_BUCKET/$NETWORK/" \
      --storage-class STANDARD_IA
fi

echo "Backup completed: backup_$TIMESTAMP.tar.gz"
echo "Backup size: $(du -h "$BACKUP_DIR/backup_$TIMESTAMP.tar.gz" | cut -f1)"
