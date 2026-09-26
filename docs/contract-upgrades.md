# Contract Upgrade Checklist

Run this checklist for every Soroban contract upgrade. The compatibility
checks protect the public interface, but they do not replace review of the
candidate WASM, migration behavior, or deployment controls.

## Before deployment

- [ ] Build the candidate WASM from a reviewed, tagged commit.
- [ ] Record the WASM hash, contract ID, network, release, and approvers.
- [ ] Compare the candidate manifest with the deployed manifest.
- [ ] Confirm every existing function signature, storage key, and error code is
      preserved; additive changes are reviewed separately.
- [ ] Run the contract upgrade test suite, including state-preservation and
      invalid-manifest cases.
- [ ] Run a pre-upgrade smoke test against a disposable deployment.
- [ ] Confirm the admin account, rollback artifact, and operator access are
      available.

## During deployment

- [ ] Take and verify an application-level state snapshot where applicable.
- [ ] Call the contract's compatibility validation before the WASM update.
- [ ] Verify the transaction result and deployed WASM hash.
- [ ] Run health checks and read representative records immediately after the
      upgrade.
- [ ] Record the transaction, operator, timestamps, metrics, and outcome in the
      audit trail.

## Rollback and follow-up

- [ ] Stop rollout if validation, health checks, state reads, or error rates
      fail.
- [ ] Restore the previously approved WASM only through the authorized admin
      process.
- [ ] Re-run health and state-preservation checks after rollback.
- [ ] Attach test output and deployment evidence to the release record.
- [ ] Review incidents and update the manifest or migration documentation.

The CI workflow
`.github/workflows/contract-upgrade-tests.yml` runs the repeatable checks for
the `ip_registry` and `atomic_swap` contracts on pull requests that change
contract code or upgrade documentation.
