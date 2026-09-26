# Disaster Recovery Procedures

> Issue #1062. Owner: Platform on-call. Review: quarterly, and after every SEV-1/SEV-2.

This guide keeps AtomicIP running, or gets it back quickly, when components fail.
It covers recovery objectives, backups, step-by-step recovery, incident response
playbooks and a DR testing schedule.

## 1. System inventory and criticality

| Component | Source of truth | State held | Tier |
|-----------|-----------------|-----------|------|
| `ip_registry` contract (Soroban) | Stellar ledger | Commitments, ownership, audit trail | **T0** |
| `atomic_swap` contract (Soroban) | Stellar ledger | Escrowed funds, swap state | **T0** |
| Contract admin / upgrade keys | HSM / offline vault | Admin authority | **T0** |
| API server (`api-server`) | Stateless (container image) | None (rebuildable) | T1 |
| Soroban RPC provider | External / self-hosted node | Ledger view | T1 |
| Redis (`REDIS_URL`) | Redis | Cache, rate limits, dedup keys, GraphQL pub/sub | T2 |
| Database (`DATABASE_URL`) | Postgres | Off-chain indexes, webhooks, sessions | T1 |
| Audit log (`AUDIT_LOG_PATH`, HMAC via `AUDIT_HMAC_KEY`) | File / object storage | Admin audit trail | T1 |
| Secrets (JWT, signing, HMAC, webhook keys) | Secret manager | Credentials | T0 |
| Monitoring (`monitoring/`) | Git + Prometheus TSDB | Metrics/alerts | T2 |

> **Key property:** the ledger is the source of truth for all IP and swap state.
> Off-chain stores are caches or indexes and can be **rebuilt from chain
> events**. The unrecoverable assets are **keys**: contract admin keys,
> and users' `secret`/`blinding_factor` (the user's own custody, see
> [ADR-0002](adr/0002-pedersen-commitments-for-privacy.md)).

## 2. RTO and RPO targets

| Tier | Scenario | RTO (time to restore) | RPO (max data loss) |
|------|----------|-----------------------|---------------------|
| T0 | Contract state | n/a: replicated by Stellar validators | 0 (ledger) |
| T0 | Loss/compromise of admin key | 4 h to rotate/freeze | 0 |
| T1 | API server region/instance loss | **30 min** | 0 (stateless) |
| T1 | Soroban RPC provider outage | **15 min** (failover) | 0 |
| T1 | Database loss | **2 h** | **5 min** (WAL/PITR) |
| T1 | Audit log loss | 4 h | **15 min** |
| T2 | Redis loss | **15 min** | Accept full loss (cache) |
| T2 | Monitoring loss | 8 h | 24 h metrics |

Service-level objective: 99.9% monthly availability of the read API, and 99.5% of
write endpoints (these depend on RPC).

## 3. Backup and restore procedures

### 3.1 Backup schedule

| Asset | Method | Frequency | Retention | Location |
|-------|--------|-----------|-----------|----------|
| Postgres | Continuous WAL archiving + base backup | WAL: continuous, base: daily 02:00 UTC | 35 days PITR, monthly for 1 year | Object storage, cross-region, encrypted (SSE-KMS) |
| Audit log | Append-only object storage sync | Every 15 min | 7 years | Versioned + object-locked bucket |
| Secrets | Secret-manager native replication + sealed export | On change | All versions | Secondary region + offline |
| Contract admin keys | Shamir split (3-of-5) paper/HSM backups | On creation/rotation | Permanent | 5 geographically separated custodians |
| Contract WASM + deploy config | Git tags + release artifacts | Every release | Permanent | GitHub releases + object storage |
| Contract IDs / network config | `IP_REGISTRY_CONTRACT`, deploy output | Every deploy | Permanent | Git (`deploy` output) + secret manager |
| Redis | RDB snapshot (optional) | Hourly | 24 h | Same region |
| Grafana/Prometheus config | Git (`monitoring/`) | Every change | Permanent | Git |

Every backup job emits `backup_last_success_timestamp_seconds{asset=...}`.
Alert if this is older than 2× the backup interval.

### 3.2 Restore: Postgres (point-in-time)

1. Declare an incident and freeze writes: scale the API server to read-only
   mode, or return `503` on write routes via the load balancer.
2. Pick a target time just before the corruption or loss event.
3. Provision a new instance from the latest base backup before that time and
   replay WAL up to the target:
   ```bash
   # Example with pgBackRest
   pgbackrest --stanza=atomicip --type=time \
     --target="2026-09-26 10:04:00+00" --target-action=promote restore
   ```
4. Verify: row counts versus the last `backup_verification` report, and spot-check
   recent `ip_id`s against the chain (`GET /ip/{id}` vs RPC `get_ip`).
5. **Reconcile from chain:** replay contract events from the ledger sequence
   stored in `indexer_cursor` (or from the target time minus 1 h) to fill the
   RPO gap. Chain data wins on any conflict.
6. Point `DATABASE_URL` at the restored instance, roll the API server, and lift the
   write freeze.
7. Record the actual RTO/RPO in the incident report.

### 3.3 Restore: Redis

Redis holds only rebuildable state. Provision a new instance, update `REDIS_URL`,
and restart the API server. The server falls back to in-process caches and
single-instance GraphQL subscriptions when Redis is unreachable. Expect a cold
cache and reset rate-limit windows. No reconciliation is needed.

### 3.4 Restore: API server

The API server is stateless. Redeploy the last known-good image tag to a healthy
region or cluster, with the same environment (`SOROBAN_RPC_URL`,
`IP_REGISTRY_CONTRACT`, `DATABASE_URL`, `REDIS_URL`, secrets). Verify
`GET /health/detailed` and run `scripts/smoke-test.sh` against the new
endpoint before moving DNS or load-balancer traffic.

### 3.5 Restore: Audit log

Restore the log from the versioned bucket to `AUDIT_LOG_PATH`. Verify the HMAC chain
with `AUDIT_HMAC_KEY`. Any gap or broken HMAC is a **security incident**
(see §5.4), not only a DR event.

### 3.6 Contract redeployment (last resort)

Contract state lives on the ledger and cannot be "restored". Redeploy only if
a contract is irrecoverably broken, for example after a bad upgrade. The upgrade
path is `validate_upgrade` + `upgrade` with a new WASM hash. See §5.3.

## 4. Recovery procedures by scenario

| # | Scenario | Detection | Procedure |
|---|----------|-----------|-----------|
| R1 | API server down | `ApiServerDown`, health checks | §3.4 |
| R2 | Soroban RPC outage | `SorobanRpcUnavailable`, `CommitmentRateDrop` | [Playbook 5.1](#playbook-soroban-rpc-outage) |
| R3 | Database loss/corruption | DB health, error-rate spike | §3.2 |
| R4 | Redis loss | `RedisUnavailable` | §3.3 |
| R5 | Bad contract upgrade | Contract invariant failures, swap failures | [Playbook 5.3](#playbook-bad-contract-upgrade) |
| R6 | Key compromise | Unexpected admin ops, `CommitmentArchivalSurge` | [Playbook 5.4](#playbook-key-compromise) |
| R7 | Indexer data loss / drift | `CommitmentStateDrift` | [Playbook 5.5](#playbook-indexer-data-loss) |
| R8 | Ledger state archival (expired TTL) | Missing entry reads | [Playbook 5.6](#playbook-ledger-state-archival) |
| R9 | Abnormal commitment activity | `CommitmentSpike` | [Playbook 5.7](#playbook-abnormal-commitment-activity) |
| R10 | Full region loss | Multiple T1 alerts in one region | Run R1 + R3 + R4 in the secondary region, then move DNS |

## 5. Incident response playbooks

### Roles and severity

| Severity | Definition | Response |
|----------|-----------|----------|
| SEV-1 | Funds at risk, key compromise, or full outage | Page immediately, incident commander (IC) assigned within 15 min, status page update within 30 min |
| SEV-2 | Write path down or major degradation | Page, IC within 30 min |
| SEV-3 | Partial degradation, workaround exists | Business hours |

Roles: **Incident Commander** (coordinates and decides), **Ops lead** (runs
the steps), **Comms lead** (status page, users), **Scribe** (timeline).

General flow for every incident: **Detect → Triage (assign SEV) → Contain →
Recover → Verify → Communicate → Post-mortem within 5 business days.**

<a id="playbook-soroban-rpc-outage"></a>
### 5.1 Soroban RPC outage (SEV-2)

1. Confirm the outage: `curl -s $SOROBAN_RPC_URL -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}'`.
2. Check Stellar network status. If the whole network is halted, communicate
   and wait. Nothing can be done off-chain.
3. If the provider is down, fail over `SOROBAN_RPC_URL` to the secondary provider or
   self-hosted node and roll the API server. The circuit breaker
   (`api-server/src/circuit_breaker.rs`) sheds load meanwhile.
4. Verify: `/health/detailed` RPC check is green and a test `get_ip` succeeds.
5. Symptom alerts (`HighErrorRate`, `CommitmentRateDrop`) are correlated under
   `SorobanRpcUnavailable` and should clear on their own. See
   [alerting-best-practices.md](alerting-best-practices.md).

### 5.2 Database outage (SEV-2)

1. Check whether this is a failover (managed HA) or a data loss event.
2. For failover: wait for promotion (≤ 2 min), then confirm the API reconnects
   (connection pool, `api-server/src/connection_pool.rs`).
3. For loss or corruption: run §3.2.

<a id="playbook-bad-contract-upgrade"></a>
### 5.3 Bad contract upgrade (SEV-1)

1. **Contain:** pause affected entry points if a pause mechanism is available.
   Otherwise disable the write routes at the API layer.
2. Identify the last known-good WASM hash from the release artifacts.
3. Run `validate_upgrade` with the known-good WASM and manifest, then
   `upgrade` to roll back. This requires the admin key quorum (§5.4 custody).
4. Verify with invariant checks and the smoke test (`scripts/smoke-test.sh`)
   against the network.
5. Reconcile any swaps that were affected during the window. Escrowed funds
   remain in the contract and can be released or cancelled via the normal paths
   once it is healthy.

<a id="playbook-key-compromise"></a>
### 5.4 Key compromise (SEV-1)

1. **Contain within 1 h:** rotate the admin with `set_admin` to a fresh key
   from the cold backup quorum (3-of-5 Shamir). Revoke compromised API
   secrets (JWT, request signing, `AUDIT_HMAC_KEY`, webhook secrets) in the
   secret manager and roll the API server.
2. Review the audit trail (`/v1/admin/audit/logs`,
   `/v1/admin/audit/suspicious-patterns`) and on-chain events for actions
   taken with the compromised key.
3. Notify affected users. Follow `SECURITY.md` disclosure policy.
4. For users' own leaked `secret`/`blinding_factor`: nothing can be reversed
   on-chain. Advise them to transfer or revoke affected IP as appropriate.

<a id="playbook-indexer-data-loss"></a>
### 5.5 Indexer data loss / state drift (SEV-3)

1. `CommitmentStateDrift` means off-chain lifecycle counts no longer match
   events.
2. Stop the indexer, reset its cursor to the last verified ledger, and replay
   events. Chain is the source of truth.
3. Compare `commitments_by_state` with a full chain scan for a sample of owners.

<a id="playbook-ledger-state-archival"></a>
### 5.6 Ledger state archival (SEV-2)

Soroban persistent entries are archived when their TTL expires
([ADR-0003](adr/0003-stellar-soroban-for-settlement.md)).

1. Identify the archived keys, for example from failing `get_ip` calls that return
   missing-entry errors.
2. Submit a `RestoreFootprint` operation for those keys, then `ExtendFootprintTTL`.
3. Run a TTL sweep job weekly to extend entries nearing expiry (below 30 days remaining).

<a id="playbook-abnormal-commitment-activity"></a>
### 5.7 Abnormal commitment activity (SEV-3, escalate if abusive)

1. Check `/v1/admin/commitments/lifecycle` and the Grafana commitment lifecycle
   dashboard. Is the spike from one owner or many?
2. Single owner: confirm per-user rate limits are applying (#983). Block the
   account at the edge if abusive.
3. Many owners: likely organic, for example a partner launch. Confirm with product and
   silence `CommitmentSpike` with an expiry and a comment.

## 6. Communication

- Status page is updated within 30 min for SEV-1 and 1 h for SEV-2, then every hour until resolved.
- Internal: `#atomicip-incidents` channel, one thread per incident.
- Post-mortem: blameless, published within 5 business days, with action items
  tracked as GitHub issues labelled `postmortem`.

## 7. DR testing schedule

| Test | Frequency | Pass criteria | Owner |
|------|-----------|---------------|-------|
| Backup restore verification (Postgres PITR to scratch instance) | **Weekly** (automated) | Restore completes, row counts and chain spot-checks match | Platform |
| Audit log restore + HMAC chain verification | Monthly | Chain intact | Security |
| Redis loss (kill instance in staging) | Monthly | Service degrades gracefully, no 5xx spike above 1% | Platform |
| RPC failover drill (staging) | Monthly | Failover < 15 min | Platform |
| API server region failover (game day) | **Quarterly** | RTO ≤ 30 min | Platform + on-call |
| Contract upgrade rollback on testnet | Quarterly | Rollback via `upgrade` succeeds, invariants hold | Contracts |
| Admin key recovery ceremony (3-of-5, testnet key) | **Semi-annually** | Quorum reconstitutes key within 4 h | Security |
| Full tabletop exercise (key compromise / SEV-1) | Semi-annually | Playbook gaps recorded and ticketed | IC rotation |
| Review of this document | Quarterly | RTO/RPO still met by latest drill results | Platform |

After each drill, record the date, participants, measured RTO/RPO and
follow-up issues in the table below.

### Drill log

| Date | Test | RTO measured | RPO measured | Issues opened |
|------|------|--------------|--------------|---------------|
| _(first drill pending)_ | | | | |

## 8. Contacts and access

Keep the live list in the on-call tool, not in this repo. Before any
incident, confirm that every on-call engineer has:

- [ ] Access to the secret manager (break-glass role)
- [ ] Access to backup buckets (read and restore)
- [ ] Deploy rights for the API server
- [ ] The key custodian contact list
- [ ] Status page publish rights
