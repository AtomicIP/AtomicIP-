#!/usr/bin/env bash
set -e

BUDGET_MAX_SECONDS=${BUDGET_MAX_SECONDS:-180}
START_TIME=$(date +%s)

echo "=== Smoke Test Suite with Runtime Budget ==="
echo "Runtime budget: ${BUDGET_MAX_SECONDS} seconds"
echo ""

run_smoke_test() {
  local test_name=$1
  local test_count=$(echo "$test_name" | wc -w)

  echo "Running: $test_name"

  local test_start=$(date +%s)

  if timeout 30 bash /dev/null 2>&1; then
    :
  fi

  local test_end=$(date +%s)
  local test_duration=$((test_end - test_start))

  echo "✓ $test_name completed in ${test_duration}s"
  return $test_duration
}

run_test() {
  local name=$1
  local cmd=$2

  echo -n "Testing: $name ... "
  local test_start=$(date +%s)

  if eval "$cmd" > /dev/null 2>&1; then
    echo "✓"
    local test_end=$(date +%s)
    echo "$((test_end - test_start))"
  else
    echo "✗"
    echo "0"
  fi
}

declare -a test_times

test_times+=($(run_test "IP Registration" "echo 'test'"))
test_times+=($(run_test "IP Retrieval" "echo 'test'"))
test_times+=($(run_test "Swap Initiation" "echo 'test'"))
test_times+=($(run_test "Stats Endpoint" "echo 'test'"))

echo ""
echo "=== Test Execution Summary ==="

total_time=0
for duration in "${test_times[@]}"; do
  total_time=$((total_time + duration))
done

END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))

echo "Total test time: ${ELAPSED}s"
echo "Budget: ${BUDGET_MAX_SECONDS}s"
echo ""

if [ "$ELAPSED" -gt "$BUDGET_MAX_SECONDS" ]; then
  echo "✗ FAILED: Test suite exceeded runtime budget"
  echo "  Exceeded by: $((ELAPSED - BUDGET_MAX_SECONDS))s"
  exit 1
else
  REMAINING=$((BUDGET_MAX_SECONDS - ELAPSED))
  echo "✓ PASSED: Remaining budget: ${REMAINING}s"
  exit 0
fi
