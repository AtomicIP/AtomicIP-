#!/usr/bin/env bash
# #1069: Roll back an environment by reverting its most recent promotion commit.
#
#   scripts/gitops-rollback.sh <staging|production> [commit-sha]
#
# Creates a revert commit on a new branch and opens a PR (production) or pushes
# directly to main (staging). Argo CD then syncs the previous image tag. No
# kubectl access is required; the cluster always follows Git.
set -euo pipefail

ENVIRONMENT="${1:?usage: $0 <staging|production> [commit-sha]}"
TARGET_SHA="${2:-}"
OVERLAY="deploy/k8s/overlays/${ENVIRONMENT}/kustomization.yaml"

case "$ENVIRONMENT" in
  staging|production) ;;
  *) echo "unknown environment: $ENVIRONMENT" >&2; exit 2 ;;
esac

git fetch origin main
if [ -z "$TARGET_SHA" ]; then
  TARGET_SHA=$(git log origin/main -1 --format=%H -- "$OVERLAY")
fi
[ -n "$TARGET_SHA" ] || { echo "no commits touch $OVERLAY" >&2; exit 1; }

echo "Reverting $(git log -1 --format='%h %s' "$TARGET_SHA")"

BRANCH="gitops/rollback-${ENVIRONMENT}-${TARGET_SHA:0:12}"
git switch -c "$BRANCH" origin/main

PARENTS=$(git rev-list --parents -n1 "$TARGET_SHA" | wc -w)
if [ "$PARENTS" -gt 2 ]; then
  # Promotion PRs merged with a merge commit: revert relative to mainline.
  git revert --no-edit -m 1 "$TARGET_SHA"
else
  git revert --no-edit "$TARGET_SHA"
fi

if [ "$ENVIRONMENT" = "production" ]; then
  git push -u origin "$BRANCH"
  if command -v gh >/dev/null 2>&1; then
    gh pr create --base main --head "$BRANCH" \
      --title "rollback(production): revert ${TARGET_SHA:0:12}" \
      --label deployment,production,rollback \
      --body "Rolls production back by reverting ${TARGET_SHA}. Argo CD will sync the previous image once merged."
  else
    echo "Pushed $BRANCH; open a PR against main to complete the rollback."
  fi
else
  git push origin "HEAD:main"
  echo "Staging rollback pushed; Argo CD will sync within its poll interval."
fi
