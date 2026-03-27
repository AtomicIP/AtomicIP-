# Git Workflow for Creating 4 Separate PRs

This guide shows how to create individual PRs for each issue from the current codebase state.

---

## Current State

All fixes are currently in a single commit/working directory. We need to split them into 4 separate PRs:
- Issue #32 (pre-existing - no changes needed)
- Issue #34 (new fix)
- Issue #35 (new fix)  
- Issue #44 (pre-existing - no changes needed)

---

## Option 1: Cherry-pick Approach (Recommended)

### Step 1: Save Current Work
```bash
# Make sure you're on main branch
git checkout main

# Create a backup branch with all current changes
git checkout -b all-fixes-backup
```

### Step 2: Create PR for Issue #34 (Payment to Seller)
```bash
# Go back to main
git checkout main

# Create new branch for issue #34
git checkout -b fix/issue-34-seller-payment

# Interactively stage only the reveal_key changes
git add -p contracts/atomic_swap/src/lib.rs
# When prompted, select only the hunk for Issue #34 (first hunk)

# Commit the change
git commit -m "Fix: Release escrowed payment to seller on reveal_key

- Transfer payment from contract escrow to seller after successful key verification
- Ensures atomic swap property: seller receives payment when revealing valid key
- Closes #34"

# Push and create PR
git push origin fix/issue-34-seller-payment
```

### Step 3: Create PR for Issue #35 (Buyer Refund)
```bash
# Go back to main
git checkout main

# Create new branch for issue #35
git checkout -b fix/issue-35-buyer-refund

# Interactively stage only the cancel_expired_swap changes
git add -p contracts/atomic_swap/src/lib.rs
# When prompted, select only the hunk for Issue #35 (second hunk)

# Commit the change
git commit -m "Fix: Refund buyer on cancel_expired_swap

- Transfer escrowed payment back to buyer when cancelling expired swap
- Implemented in cancel_expired_swap() for Accepted swaps
- Protects buyers from losing funds if seller doesn't reveal key
- Closes #35"

# Push and create PR
git push origin fix/issue-35-buyer-refund
```

### Step 4: Document Pre-existing Fixes (#32 and #44)
```bash
# Go back to main
git checkout main

# Create documentation branch for pre-existing fixes
git checkout -b docs/issues-32-44-verification

# The fixes already exist in the codebase, just document them
git add ISSUES_FIX_SUMMARY.md PR_GUIDE.md
git commit -m "Docs: Add verification of pre-existing fixes for #32 and #44

- Document that issue #32 (key verification) was already implemented
- Document that issue #44 (duplicate check) was already implemented
- Add comprehensive test coverage analysis
- Closes #32, Closes #44"

# Push (optional - mainly for documentation)
git push origin docs/issues-32-44-verification
```

---

## Option 2: Reset and Rebuild Approach

If you prefer a cleaner approach:

### Step 1: Reset All Changes
```bash
git checkout main
git reset --hard HEAD
```

### Step 2: Apply Fix #34 Only
```bash
# Edit the file to add only the seller payment fix
# Use your editor to add lines 202-207 in contracts/atomic_swap/src/lib.rs

git add contracts/atomic_swap/src/lib.rs
git commit -m "Fix: Release escrowed payment to seller on reveal_key

Closes #34"

git push origin fix/issue-34-seller-payment
```

### Step 3: Apply Fix #35 Only
```bash
# Edit the file to add only the buyer refund fix
# Use your editor to add lines 286-291 in contracts/atomic_swap/src/lib.rs

git add contracts/atomic_swap/src/lib.rs
git commit -m "Fix: Refund buyer on cancel_expired_swap

Closes #35"

git push origin fix/issue-35-buyer-refund
```

---

## Option 3: Single Combined PR (Fallback)

If maintaining separate PRs is too complex:

```bash
# From your current state with all changes
git checkout -b fix/all-atomicip-issues

# Commit all changes together
git add contracts/atomic_swap/src/lib.rs
git commit -m "Fix: Multiple atomic swap security issues

- Fix #34: Release escrowed payment to seller on reveal_key
- Fix #35: Refund buyer on cancel_expired_swap
- Document pre-existing fixes: #32 (key verification) and #44 (duplicate check)

Closes #32, #34, #35, #44"

git push origin fix/all-atomicip-issues
```

Then mention in the PR description that this could be split into separate PRs if preferred.

---

## Verifying Each PR

After creating each branch, verify the changes:

```bash
# Check what files were changed
git diff HEAD~1

# Verify only the intended changes are present
git show --stat

# Build to ensure no compilation errors
cargo build --package atomic_swap --lib

# Run tests if disk space allows
cargo test --package atomic_swap
```

---

## GitHub PR Creation

For each branch, create a PR on GitHub:

1. Go to https://github.com/AtomicIP/AtomicIP-/compare
2. Select the branch you just pushed
3. Use the PR description templates from `PR_GUIDE.md`
4. Reference the issue number with "Closes #XX"
5. Submit the PR

---

## Recommended Order

Submit PRs in this order for logical progression:

1. **PR #34** - Seller payment (core functionality)
2. **PR #35** - Buyer refund (completes the escrow flow)
3. **PR #32** - Documentation only (verify pre-existing fix)
4. **PR #44** - Documentation only (verify pre-existing fix)

Alternatively, skip documentation PRs and just reference #32 and #44 as already fixed in the code review comments.

---

## Quick Reference Commands

```bash
# See current uncommitted changes
git status
git diff

# View specific hunks
git diff contracts/atomic_swap/src/lib.rs | grep -A 10 -B 5 "Issue #"

# Stash all changes temporarily
git stash

# Restore stashed changes
git stash pop

# Abort if something goes wrong
git reset --hard main
```

---

## Important Notes

1. **Issues #32 and #44** are already fixed in the codebase - no code changes needed
2. **Issues #34 and #35** required new code which has been added
3. All 4 issues can be referenced in commits even if only 2 required code changes
4. Consider adding explicit tests for #35 (buyer refund) as it's not currently tested
5. The verification script `verify_fixes.sh` can validate all fixes once compiled
