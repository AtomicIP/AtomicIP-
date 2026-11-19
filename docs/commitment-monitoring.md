# Commitment Lifecycle Monitoring

> Issue #1064

## States

| State | Entered by | Meaning |
|-------|-----------|---------|
| `active` | `commit_ip`, `batch_commit_ip*`, `create_time_locked_commitment` | Commitment is on-chain and hidden |
| `revealed` | `unlock_commitment`, `reveal_partial`, swap `reveal_key` | Secret (or part of it) is disclosed |
| `archived` | `revoke_ip`, expiry | Commitment can no longer be swapped |

Legal edges: `new→active`, `active→revealed`, `active→archived`,
`revealed→archived`. Any other edge increments
`commitment_invalid_transitions_total` and raises `CommitmentInvalidTransition`.

## Metrics

| Metric | Type | Labels |
|--------|------|--------|
| `commitments_by_state` | gauge | `state` |
| `commitment_transitions_total` | counter | `from`, `to` |
| `commitment_transition_rate_per_minute` | gauge | `from`, `to` |
| `commitment_transition_trend` | gauge (least-squares slope per bucket) | `from`, `to` |
| `commitment_invalid_transitions_total` | counter | `from`, `to` |

The JSON snapshot is at `GET /v1/admin/commitments/lifecycle`.

## Anomaly detection

Every 30s a background task (`commitment_monitoring::spawn_background_evaluator`)
compares the last completed 1-minute bucket against the previous buckets (up to
60) using a z-score:

- `z ≥ 3` and count ≥ 10 → `CommitmentSpike` / `CommitmentTransitionSpike`
- `z ≤ −3` and baseline mean ≥ 10 → `CommitmentRateDrop` / `CommitmentTransitionDrop`
- any negative state gauge → `CommitmentStateDrift` (critical)

Alerts go through the alert-fatigue pipeline in `alerting.rs` (see
[alerting-best-practices.md](alerting-best-practices.md)). The same conditions
also exist as Prometheus rules in `monitoring/prometheus/commitment-lifecycle-rules.yml`
so they work across multiple replicas.

## Trend analysis

- Short-term: `commitment_transition_trend` slope, classified in the JSON
  snapshot as `increasing` / `decreasing` / `stable` (±5% of the mean per bucket).
- Long-term: `atomicip:commitment_transitions:wow_ratio` (week-over-week) and the
  `CommitmentTrendDecline` info alert.

## Dashboards

Import `monitoring/grafana/commitment-lifecycle-dashboard.json`. It has one row
per state (count, inflow, outflow), plus trend and alert-pipeline rows.

## Limitations

- In-process counts are per replica and reset on restart. Use the Prometheus
  recording rules for fleet-wide views.
- Only API-originated commits are currently instrumented. Revealed/archived
  transitions should be fed from the contract event indexer
  (`record_transition(Active, Archived)` on `ip_revoked`, etc.).

## Runbooks

Each alert in `monitoring/prometheus/commitment-lifecycle-rules.yml` links here
or to a playbook in `docs/disaster-recovery.md` via its `runbook_url`
annotation.

### Runbook: invalid transition

<a id="runbook-invalid-transition"></a>

**Alert:** `CommitmentInvalidTransition` (metric `commitment_invalid_transitions_total`)

An on-chain lifecycle transition was observed that the state machine in
[States](#states) does not allow (for example `archived` → `active`). The
indexer has either received an event out of order or the contract emitted an
unexpected state change.

1. Open the Prometheus query `increase(commitment_invalid_transitions_total[10m])`
   and note the `from` and `to` labels of the series that fired.
2. Find the affected commitment IDs in the indexer logs for the same window.
3. Re-read those commitments' state from Soroban RPC and compare it with the
   indexer's database row.
4. If the on-chain state is correct and the indexer is wrong, replay the
   commitment's events from the last good ledger (see
   [Indexer data loss / state drift](disaster-recovery.md#playbook-indexer-data-loss)).
5. If the on-chain state itself violates the lifecycle, treat it as a contract
   bug: follow [Bad contract upgrade](disaster-recovery.md#playbook-bad-contract-upgrade)
   and open a SEV-1 incident.
6. Record the outcome in the incident log before resolving the alert.

### Runbook: trend decline

<a id="runbook-trend-decline"></a>

**Alert:** `CommitmentTrendDecline` (recording rule `atomicip:commitment_transitions:wow_ratio`)

New commitment creation has fallen below half of the value from one week
earlier, sustained for 6 hours. This is usually a demand change, but it can also
mean users cannot reach the service.

1. Check the commitment creation rate on the [Dashboards](#dashboards) for the
   last 24 hours to confirm the drop is real and not a metrics gap.
2. Check whether `CommitmentRateDrop` or `SorobanRpcUnavailable` is also firing.
   If so, follow [Soroban RPC outage](disaster-recovery.md#playbook-soroban-rpc-outage)
   first; this alert is then a symptom.
3. Check the frontend and API error rates for the same window.
4. If there is no technical cause, record the observation and review it at the
   next weekly metrics review. Do not change alert thresholds to silence it.
