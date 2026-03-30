# Changelog Format

Releases use `git-cliff` to generate release notes from commit messages and pull request metadata.

## Source Data

- Conventional Commit messages in git history
- Pull Request references attached to commits (for example, `(#123)` or merge commits)
- GitHub author metadata when available

## Grouping Rules

Entries are grouped by commit type:

- Features: `feat`
- Fixes: `fix`
- Performance: `perf`
- Refactoring: `refactor`
- Documentation: `docs`
- Tests: `test`
- Build: `build`
- CI: `ci`
- Chores: `chore`
- Security: any commit body containing `security`
- Other: fallback for unmatched commits

## Release Trigger

The release workflow runs automatically when a tag matching `v*` is pushed.
It can also be run manually through `workflow_dispatch`.

## Output

For each release, generated notes follow this structure:

1. Version header with date
2. Grouped sections (Features, Fixes, and so on)
3. One bullet per commit with optional PR number and GitHub username

Example line:

`- Fix escrow timeout edge case (#142) by @contributor`
