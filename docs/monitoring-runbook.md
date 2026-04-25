# Monitoring Runbook

## Common Alerts and Response Procedures

### APIDown

**Severity**: Critical

**Description**: The API server is not responding to health checks.

**Response Steps**:
1. Check if the API server process is running: `systemctl status atomicip-api`
2. Check recent logs: `journalctl -u atomicip-api -n 100`
3. Verify network connectivity and firewall rules
4. Check resource usage (CPU, memory, disk)
5. Restart the service if necessary: `systemctl restart atomicip-api`
6. If restart fails, check for configuration errors
7. Escalate to on-call engineer if issue persists > 5 minutes

**Prevention**: Implement health checks, auto-restart policies, and resource monitoring.

---

### HighErrorRate

**Severity**: Critical

**Description**: API is returning 5xx errors at an elevated rate.

**Response Steps**:
1. Check error logs for specific error messages
2. Identify affected endpoints: check Grafana dashboard
3. Check blockchain node connectivity
4. Verify database/storage availability
5. Check for recent deployments or configuration changes
6. If caused by bad deployment, rollback immediately
7. If infrastructure issue, scale resources or failover

**Prevention**: Implement canary deployments, comprehensive testing, and circuit breakers.

---

### ContractPaused

**Severity**: Warning

**Description**: The atomic swap contract has been paused.

**Response Steps**:
1. Verify if pause was intentional (check change log)
2. If unintentional, investigate who paused and why
3. Check for security incidents or exploits
4. Review recent contract activity for anomalies
5. If safe to resume, unpause contract via admin function
6. Notify users of service restoration

**Prevention**: Implement pause authorization controls and audit logging.

---

### HighLatency

**Severity**: Warning

**Description**: API response times are elevated above normal thresholds.

**Response Steps**:
1. Check current load and traffic patterns
2. Identify slow endpoints in Grafana
3. Check blockchain node performance
4. Review database query performance
5. Check for resource constraints (CPU, memory, network)
6. Consider scaling horizontally if sustained high load
7. Optimize slow queries or endpoints if identified

**Prevention**: Load testing, query optimization, caching, and auto-scaling.

---

### LowCompletionRate

**Severity**: Warning

**Description**: Fewer swaps are completing successfully than expected.

**Response Steps**:
1. Check for increased cancellations or timeouts
2. Review recent swap failures in logs
3. Check if buyers are having payment issues
4. Verify reveal key mechanism is working
5. Check for UI/UX issues preventing completion
6. Analyze user feedback and support tickets
7. Investigate contract logic if pattern is unusual

**Prevention**: User education, better UX, timeout tuning, and monitoring user journeys.

---

## General Troubleshooting

### Checking Logs
```bash
# API server logs
journalctl -u atomicip-api -f

# Contract events
stellar-cli events --id <contract-id> --start-ledger <ledger>
```

### Checking Metrics
```bash
# Query Prometheus directly
curl http://localhost:9090/api/v1/query?query=up

# Check API metrics endpoint
curl http://localhost:8080/metrics
```

### Emergency Contacts
- On-call Engineer: [contact info]
- DevOps Lead: [contact info]
- Security Team: [contact info]

### Escalation Path
1. On-call engineer (0-15 min)
2. DevOps lead (15-30 min)
3. Engineering manager (30+ min)
4. CTO (critical incidents only)
