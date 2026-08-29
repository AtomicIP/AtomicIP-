#!/usr/bin/env bash
# Fails the build if a `#[cfg(test)] mod ...;` declaration has been commented
# out alongside a FIXME marker (see #804) — that pattern hides a test module
# from CI silently, so it must not slip back in unnoticed.
#
# Scoped to contracts/ip_registry, the crate #804 covers. Other crates (e.g.
# atomic_swap) carry their own pre-existing disabled-test debt tracked
# separately and are out of scope here.
set -euo pipefail

TARGET_DIR="contracts/ip_registry"
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

echo "No disabled test modules found."
