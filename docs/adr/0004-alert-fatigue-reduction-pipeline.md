# ADR-0004: Alert fatigue reduction pipeline

- **Status:** Accepted
- **Date:** 2026-09-26
- **Related issues / PRs:** #1064, #1065

## Context

Per-path error alerts and lifecycle anomaly alerts fire in bursts when one
upstream dependency (Soroban RPC, Redis) fails. This paged on-call many
times for a single root cause.

## Options considered

- **A. Rely only on Alertmanager.** Handles fleet-wide Prometheus alerts, but not
  alerts raised inside the API server (e.g. invalid lifecycle transitions).
- **B. An in-process pipeline plus Alertmanager, using the same model.**
- **C. A third-party incident platform.** New vendor dependency and cost.

## Decision

Option B. `api-server/src/alerting.rs` implements silence → dedup → correlate →
escalate. `monitoring/alertmanager/alertmanager.yml` mirrors it with
`group_by`, `inhibit_rules`, silences and severity routes. Correlation rules
must be kept in sync in both places.

## Consequences

- One page per incident instead of one per symptom.
- Two configurations to keep in sync (documented in
  `docs/alerting-best-practices.md`).
- In-process state is per replica. Fleet-wide dedup relies on Alertmanager.
