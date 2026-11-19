#!/usr/bin/env bash
# Fails if a third-party GitHub Action in .github/workflows is pinned to a
# mutable tag or branch instead of a full 40-character commit SHA.
# Actions owned by GitHub ("actions/") and local ("./") actions are exempt.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKFLOWS="$ROOT/.github/workflows"

status=0
while IFS= read -r line; do
  file="${line%%:*}"
  rest="${line#*:}"
  lineno="${rest%%:*}"
  body="${rest#*:}"
  # Extract the value after "uses:" and drop any trailing comment.
  ref="$(printf '%s' "$body" | sed -E 's/^[[:space:]]*(-[[:space:]]+)?uses:[[:space:]]*//; s/[[:space:]]*#.*$//; s/[[:space:]]+$//; s/^["'\'']//; s/["'\'']$//')"
  case "$ref" in
    ./*|docker://*) continue ;;
    actions/*) continue ;;
  esac
  sha="${ref##*@}"
  if [[ "$ref" != *@* ]] || ! [[ "$sha" =~ ^[0-9a-f]{40}$ ]]; then
    echo "${file#$ROOT/}:$lineno: third-party action must be pinned to a commit SHA: $ref"
    status=1
  fi
done < <(grep -nE '^[[:space:]]*(-[[:space:]]+)?uses:' "$WORKFLOWS"/*.yml | sed -E 's/^([^:]+):([0-9]+):/\1:\2:/')

if [[ $status -eq 0 ]]; then
  echo "All third-party action pins use commit SHAs"
fi
exit $status
