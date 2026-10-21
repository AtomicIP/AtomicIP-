#!/usr/bin/env bash
# Fails the build if a `#[cfg(test)] mod ...;` declaration has been commented
# out alongside a FIXME marker — that pattern hides a test module from CI
# silently, so it must not slip back in unnoticed.
#
# #884: Extended from ip_registry-only scope to cover ALL contracts under
# the `contracts/` directory.  Newly-added or re-enabled modules in any
# contract crate are now checked automatically.
#
# Individual known-disabled modules that carry a legitimate long-term FIXME
# (e.g. benchmarks.rs, invariant_tests.rs in ip_registry) are tracked via
# the CI "Check for stale merge-conflict FIXMEs" step in ci.yml, which
# maintains a per-file allowlist.  This script only checks that the *count*
# of such FIXMEs does not increase.
#
# #1228: Also fails when a NEW `#[ignore]` attribute is added to a test
# without a linked issue reference (e.g. a comment containing `#<number>`
# or `issue #<number>` on the same or preceding line).  Existing ignored
# tests are captured in a baseline file so only newly added ignores without
# a linked issue cause failure.
set -euo pipefail

TARGET_DIR="contracts"
found=0

# ---------------------------------------------------------------------------
# Check 1: FIXME markers preceding commented-out test modules
# ---------------------------------------------------------------------------
while IFS= read -r entry; do
  file="${entry%%:*}"
  line="${entry#*:}"
  line="${line%%:*}"
  next_line=$(sed -n "$((line + 1))p" "$file")
  if echo "$next_line" | grep -qE '^\s*//\s*(#\[cfg\(test\)\]|mod\s)'; then
    echo "$file:$line: FIXME marker precedes a commented-out test module"
    found=1
  fi
done < <(grep -rn --include='*.rs' -E '//\s*FIXME' "$TARGET_DIR")

# ---------------------------------------------------------------------------
# Check 2: Newly added `#[ignore]` attributes without a linked issue
# ---------------------------------------------------------------------------
# Baseline file lists the pre-existing ignored tests (one per line, in the
# form `<file>:<line>`).  Any `#[ignore]` not present in the baseline is
# considered "new" and must carry a linked issue reference.
IGNORE_BASELINE="${IGNORE_BASELINE:-scripts/ignored-tests-baseline.txt}"

# Collect all current `#[ignore]` occurrences as `<file>:<line>`.
current_ignores=$(grep -rn --include='*.rs' -E '^\s*#\[ignore' "$TARGET_DIR" \
  | awk -F: '{print $1":"$2}' || true)

# Load baseline (if present) into an associative array.
declare -A baseline_set=()
if [ -f "$IGNORE_BASELINE" ]; then
  while IFS= read -r bline; do
    # Skip blank lines and comments.
    [ -z "$bline" ] && continue
    case "$bline" in
      \#*) continue ;;
    esac
    baseline_set["$bline"]=1
  done < "$IGNORE_BASELINE"
fi

# Iterate over current ignores and validate any that are not in the baseline.
while IFS= read -r entry; do
  [ -z "$entry" ] && continue
  file="${entry%%:*}"
  line="${entry##*:}"

  # Skip if this ignore is part of the known baseline.
  if [ -n "${baseline_set[$entry]:-}" ]; then
    continue
  fi

  # Look for a linked issue reference on the same line or the preceding line.
  same_line=$(sed -n "${line}p" "$file")
  prev_line=""
  if [ "$line" -gt 1 ]; then
    prev_line=$(sed -n "$((line - 1))p" "$file")
  fi

  if echo "$same_line" | grep -qE '(#|issue[[:space:]]*#[[:space:]]*)[0-9]+' \
     || echo "$prev_line" | grep -qE '(#|issue[[:space:]]*#[[:space:]]*)[0-9]+'; then
    continue
  fi

  echo "$file:$line: new #[ignore] attribute without a linked issue reference"
  found=1
done <<< "$current_ignores"

if [ "$found" -eq 1 ]; then
  exit 1
fi

echo "No new disabled test modules or unlinked #[ignore] attributes found in $TARGET_DIR."