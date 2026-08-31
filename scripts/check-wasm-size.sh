#!/usr/bin/env bash
set -e

CONTRACTS_DIR="contracts"
WASM_DIR="target/wasm32-unknown-unknown/release"
BASELINE_FILE="scripts/wasm-size-baseline.json"
REGRESSION_THRESHOLD=${REGRESSION_THRESHOLD:-10}

echo "=== Checking WASM Size Regressions ==="

contracts=("ip_registry" "atomic_swap")
declare -A sizes
declare -A baselines

if [ ! -f "$BASELINE_FILE" ]; then
  echo "Baseline file not found at $BASELINE_FILE"
  echo "Creating initial baseline..."
  mkdir -p "$(dirname "$BASELINE_FILE")"
  echo '{}' > "$BASELINE_FILE"
fi

baselines=$(cat "$BASELINE_FILE")

for contract in "${contracts[@]}"; do
  wasm_file="$WASM_DIR/${contract}.wasm"

  if [ ! -f "$wasm_file" ]; then
    echo "Warning: WASM file not found for $contract at $wasm_file"
    continue
  fi

  size=$(stat -f%z "$wasm_file" 2>/dev/null || stat -c%s "$wasm_file" 2>/dev/null)
  sizes[$contract]=$size

  baseline_size=$(echo "$baselines" | grep -o "\"$contract\":[0-9]*" | cut -d: -f2 || echo "0")

  if [ "$baseline_size" -gt 0 ]; then
    percent_change=$(( (size - baseline_size) * 100 / baseline_size ))

    if [ "$percent_change" -gt "$REGRESSION_THRESHOLD" ]; then
      echo "❌ REGRESSION DETECTED: $contract"
      echo "   Baseline: $baseline_size bytes"
      echo "   Current:  $size bytes"
      echo "   Change:   +${percent_change}%"
      exit 1
    elif [ "$percent_change" -lt "-$REGRESSION_THRESHOLD" ]; then
      echo "✅ IMPROVEMENT: $contract"
      echo "   Baseline: $baseline_size bytes"
      echo "   Current:  $size bytes"
      echo "   Change:   ${percent_change}%"
    else
      echo "✓ OK: $contract ($size bytes, change: ${percent_change}%)"
    fi
  else
    echo "ℹ First baseline for $contract: $size bytes"
  fi
done

echo ""
echo "WASM Size Check Complete"
