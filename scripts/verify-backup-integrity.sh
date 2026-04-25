#!/bin/bash
# verify-backup-integrity.sh
# Verify backup file integrity and contents

set -e

BACKUP_FILE="$1"

if [ -z "$BACKUP_FILE" ]; then
    echo "Usage: $0 <backup_file.tar.gz>"
    exit 1
fi

if [ ! -f "$BACKUP_FILE" ]; then
    echo "Error: Backup file not found: $BACKUP_FILE"
    exit 1
fi

echo "Verifying backup: $BACKUP_FILE"
echo "File size: $(du -h "$BACKUP_FILE" | cut -f1)"
echo ""

# Test archive integrity
echo "Testing archive integrity..."
if tar -tzf "$BACKUP_FILE" > /dev/null 2>&1; then
    echo "✓ Archive is valid"
else
    echo "✗ Archive is corrupted"
    exit 1
fi

# Extract to temp directory for inspection
TEMP_DIR="/tmp/backup_verify_$$"
mkdir -p "$TEMP_DIR"
tar -xzf "$BACKUP_FILE" -C "$TEMP_DIR"

# Find backup data directory
BACKUP_DIR=$(find "$TEMP_DIR" -mindepth 1 -maxdepth 1 -type d | head -1)

# Check required files
echo ""
echo "Checking required files..."

REQUIRED_FILES=(
    "metadata.json"
    "ip_registry_state.json"
    "atomic_swap_state.json"
)

ALL_PRESENT=true
for file in "${REQUIRED_FILES[@]}"; do
    if [ -f "$BACKUP_DIR/$file" ]; then
        SIZE=$(du -h "$BACKUP_DIR/$file" | cut -f1)
        echo "✓ $file ($SIZE)"
    else
        echo "✗ $file (missing)"
        ALL_PRESENT=false
    fi
done

# Validate JSON files
echo ""
echo "Validating JSON structure..."
for json_file in "$BACKUP_DIR"/*.json; do
    if [ -f "$json_file" ]; then
        if jq empty "$json_file" 2>/dev/null; then
            echo "✓ $(basename "$json_file") is valid JSON"
        else
            echo "✗ $(basename "$json_file") is invalid JSON"
            ALL_PRESENT=false
        fi
    fi
done

# Cleanup
rm -rf "$TEMP_DIR"

echo ""
if [ "$ALL_PRESENT" = true ]; then
    echo "✓ Backup verification passed"
    exit 0
else
    echo "✗ Backup verification failed"
    exit 1
fi
