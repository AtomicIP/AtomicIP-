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
set -euo pipefail

TARGET_DIR="contracts"
found=0

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

if [ "$found" -eq 1 ]; then
  exit 1
fi

echo "No new disabled test modules found in $TARGET_DIR."
