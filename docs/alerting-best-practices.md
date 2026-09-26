# Alerting Best Practices

> Issue #1065: Add alert fatigue reduction.

Too many alerts train people to ignore them. Every page must be **actionable,
urgent and novel**. This document describes how AtomicIP reduces alert noise
and how to write and operate alerts.

## Architecture

Alerts flow through two layers that use the same model:

| Stage | In-process (`api-server/src/alerting.rs`) | Prometheus/Alertmanager (`monitoring/`) |
|-------|-------------------------------------------|-----------------------------------------|
| Deduplication | Fingerprint = `name` + labels; repeats within `dedup_window` (5 min) only bump `occurrences` | `group_by`, `group_interval`, `repeat_interval` |
| Correlation | `CorrelationRule` folds symptom alerts into a firing root-cause alert | `inhibit_rules` |
| Silencing | `AlertManager::add_silence` with label matchers and a time window | `amtool silence add` |
| Escalation | Time-based `EscalationPolicy` (slack → pagerduty → phone) plus recurrence-based severity bump | Severity routes with shorter `repeat_interval` for critical |

Pipeline order is **silence → dedup → correlate → notify/escalate**. A silenced
alert is never counted as a new occurrence; a correlated alert never pages on
its own.

Pipeline metrics (see the “Alert pipeline” row of the Grafana dashboard):

- `alerts_notified_total{alertname,severity}`
- `alerts_deduplicated_total{alertname}`
- `alerts_correlated_total{alertname}`
- `alerts_silenced_total{alertname}`
- `alerts_escalated_total{alertname,channel}`

A healthy ratio is **≥ 5 suppressed per notified**. If `notified` dominates,
the rules are too fine-grained.

## Deduplication

- The fingerprint deliberately excludes the summary text so “12 errors” and
  “13 errors” deduplicate.
- Labels **are** part of the fingerprint. Do not put high-cardinality values
  (request IDs, user addresses, timestamps) in labels; put them in the summary.
- A resolved alert that re-fires starts a new incident.

## Correlation rules

Default rules (`alerting::default_correlation_rules`):

| Rule | Root cause | Suppressed symptoms | Grouped by |
|------|-----------|---------------------|------------|
| `rpc-outage-cascade` | `SorobanRpcUnavailable` | `HighErrorRate`, `HighLatency`, `CommitmentRateDrop`, `SwapCompletionStalled` | `network` |
| `cache-outage-cascade` | `RedisUnavailable` | `HighLatency`, `RateLimiterDegraded` | — |
| `commitment-anomaly-cascade` | `CommitmentSpike` | `CommitmentRateLimitHits`, `HighErrorRate` | — |

When adding a rule, keep the Alertmanager `inhibit_rules` in sync.

## Silencing

Silence only with a reason and an expiry. Never create open-ended silences.

```bash
# Planned maintenance of the testnet RPC node for 2 hours
amtool silence add network=testnet alertname=~"SorobanRpcUnavailable|HighLatency" \
  --duration 2h --author "$USER" --comment "RPC node upgrade, CHG-1234"
```

In-process equivalent:

```rust
alerting::ALERT_MANAGER.add_silence(Silence {
    id: "chg-1234".into(),
    matchers: vec![LabelMatcher { key: "network".into(), value: "testnet".into() }],
    starts_at: now, ends_at: now + 2 * 3600,
    created_by: "oncall".into(), comment: "RPC node upgrade".into(),
});
```

A silence with no matchers matches nothing, so a typo can't mute everything.

## Escalation

Default `EscalationPolicy`:

| Step | After | Channel |
|------|-------|---------|
| 0 | immediately | Slack `#atomicip-alerts` |
| 1 | 15 min unacknowledged | PagerDuty primary |
| 2 | 30 min further | Phone / secondary on-call (severity forced to `critical`) |

Also, an alert that keeps firing is bumped one severity level every
`recurrence_threshold` (20) occurrences. This catches flapping problems that
never stay firing long enough for time-based escalation.

Acknowledging an alert stops escalation for it and all its correlated
children. Resolving closes them all.

## Writing good alerts

1. **Alert on symptoms, not causes.** Page on “commitments failing” instead of
   “CPU 80%”. Use cause metrics on dashboards.
2. **Every alert needs a runbook.** Set `annotations.runbook` to a section in
   `docs/disaster-recovery.md` or another playbook.
3. **Use `for:`.** No alert should fire on a single scrape. The minimum is 5m for
   warnings and 1–2m for critical availability alerts.
4. **Pick severity by required response time.**
   - `critical`: user impact now, page 24/7.
   - `warning`: needs attention within business hours, Slack only.
   - `info`: goes into the daily digest, never interrupts anyone.
5. **Set absolute floors on ratio alerts.** A 3× spike on 1 req/min is noise.
   (`min_spike_count` in `commitment_monitoring`, `and ... > N` in PromQL.)
6. **Keep labels low-cardinality.** See Deduplication above.
7. **Review alerts regularly.** Any alert that fired more than 10 times in
   a week without action gets tuned or deleted in the weekly ops review.

## Review checklist for new alerts

- [ ] Actionable: the runbook tells the responder what to do
- [ ] Severity matches the required response time
- [ ] `for:` duration set
- [ ] Absolute floor on rate/ratio conditions
- [ ] Correlation/inhibition considered for known cascades
- [ ] Labels are low cardinality
- [ ] Added to the Grafana dashboard
