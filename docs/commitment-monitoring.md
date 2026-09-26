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
