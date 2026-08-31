#!/usr/bin/env bash
set -e

COVERAGE_CONFIG="scripts/coverage-config.json"
COVERAGE_FLOOR=$(jq -r '.coverage_floor' "$COVERAGE_CONFIG")
LCOV_REPORT=$(jq -r '.artifacts.lcov_report' "$COVERAGE_CONFIG")
SUMMARY_FILE=$(jq -r '.artifacts.summary' "$COVERAGE_CONFIG")

echo "=== Coverage Threshold Gate ==="
echo "Minimum required coverage: ${COVERAGE_FLOOR}%"
echo ""

if [ ! -f "$LCOV_REPORT" ]; then
  echo "Error: LCOV report not found at $LCOV_REPORT"
  echo "Make sure to run coverage collection first: cargo llvm-cov"
  exit 1
fi

total_coverage=$(grep "^  <div class=\"percentage\">" "$LCOV_REPORT" 2>/dev/null | head -1 | sed 's/.*>\([0-9.]*\).*/\1/' || echo "0")

if [ -z "$total_coverage" ] || [ "$total_coverage" = "0" ]; then
  echo "Warning: Could not extract coverage percentage from LCOV report"
  echo "Attempting alternative extraction method..."
  total_coverage=$(grep -oP '(?<=<span class="coverage">)[0-9.]+' "$LCOV_REPORT" | head -1 || echo "0")
fi

if (( $(echo "$total_coverage >= $COVERAGE_FLOOR" | bc -l) )); then
  echo "✓ Coverage requirement met: ${total_coverage}% >= ${COVERAGE_FLOOR}%"
  exit 0
else
  echo "✗ Coverage below threshold: ${total_coverage}% < ${COVERAGE_FLOOR}%"
  exit 1
fi
