# Monitoring and Alerting Guide

## Overview

This guide covers the monitoring and alerting setup for the AtomicIP platform, including contract health, API availability, and error rate tracking.

## Metrics Collection

### Prometheus Setup

The platform exposes metrics in Prometheus format at `/metrics` endpoint on the API server.

#### Key Metrics

- **Swap Volume**: `atomic_swap_total`, `atomic_swap_completed`, `atomic_swap_cancelled`
- **Fee Revenue**: `atomic_swap_fees_collected_total`
- **API Latency**: `http_request_duration_seconds`
- **Error Rates**: `http_requests_total{status="5xx"}`, `contract_errors_total`
- **Contract State**: `contract_paused`, `active_swaps_count`

### Configuration

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'atomicip-api'
    scrape_interval: 15s
    static_configs:
      - targets: ['localhost:8080']
```

## Grafana Dashboards

### Dashboard: Swap Activity
- Swap volume over time
- Completion rate vs cancellation rate
- Average swap duration
- Active swaps gauge

### Dashboard: Revenue Tracking
- Total fees collected
- Fee revenue by time period
- Average fee per swap

### Dashboard: System Health
- API request rate
- API latency (p50, p95, p99)
- Error rate by endpoint
- Contract pause status

### Dashboard: Error Analysis
- Error trends over time
- Top error types
- Failed transaction breakdown

## Alerting Rules

### Critical Alerts

#### API Down
```yaml
- alert: APIDown
  expr: up{job="atomicip-api"} == 0
  for: 1m
  labels:
    severity: critical
  annotations:
    summary: "AtomicIP API is down"
    description: "API has been unreachable for 1 minute"
```

#### High Error Rate
```yaml
- alert: HighErrorRate
  expr: rate(http_requests_total{status=~"5.."}[5m]) > 0.05
  for: 5m
  labels:
    severity: critical
  annotations:
    summary: "High error rate detected"
    description: "Error rate is {{ $value }} errors/sec"
```

#### Contract Paused
```yaml
- alert: ContractPaused
  expr: contract_paused == 1
  for: 1m
  labels:
    severity: warning
  annotations:
    summary: "Contract is paused"
    description: "Atomic swap contract has been paused"
```

### Warning Alerts

#### Elevated Latency
```yaml
- alert: HighLatency
  expr: histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[5m])) > 2
  for: 10m
  labels:
    severity: warning
  annotations:
    summary: "API latency is elevated"
    description: "P95 latency is {{ $value }}s"
```

#### Low Swap Completion Rate
```yaml
- alert: LowCompletionRate
  expr: rate(atomic_swap_completed[1h]) / rate(atomic_swap_total[1h]) < 0.7
  for: 30m
  labels:
    severity: warning
  annotations:
    summary: "Swap completion rate is low"
    description: "Only {{ $value | humanizePercentage }} of swaps completing"
```

## Runbook

See [monitoring-runbook.md](./monitoring-runbook.md) for detailed response procedures.

## Setup Instructions

1. Install Prometheus and Grafana
2. Configure Prometheus to scrape API metrics
3. Import Grafana dashboards from `monitoring/dashboards/`
4. Configure alert manager with notification channels
5. Test alerts with synthetic failures

## Maintenance

- Review and update alert thresholds quarterly
- Archive old metrics data after 90 days
- Update dashboards based on operational needs
