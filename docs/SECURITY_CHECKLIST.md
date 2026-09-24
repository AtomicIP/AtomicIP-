# Security Checklist - Quick Reference

Quick reference checklists for security-critical operations.

## Pre-Production Deployment Checklist

**Duration**: ~2 hours  
**Owner**: Security team + DevOps

### Code Security (30 min)
- [ ] All code reviewed and approved
- [ ] No hardcoded secrets or credentials
- [ ] No debugging code or console.logs
- [ ] Error messages sanitized (no stack traces)
- [ ] Input validation on all endpoints
- [ ] Request signatures verified
- [ ] No deprecated cryptography

### Dependencies (15 min)
- [ ] `npm audit` has zero high/critical vulnerabilities
- [ ] All dependencies up-to-date
- [ ] No dev dependencies in production build
- [ ] Lock file committed and consistent

### Testing (30 min)
- [ ] Unit test coverage ≥70%
- [ ] Security regression tests pass
- [ ] Load test baseline meets requirements
- [ ] Integration tests pass
- [ ] No skipped or disabled tests

### Configuration (30 min)
- [ ] Environment variables configured
- [ ] Database encrypted and hardened
- [ ] TLS 1.3+ enabled
- [ ] Rate limiting configured
- [ ] CORS properly configured
- [ ] Security headers present

### Secrets & Keys (15 min)
- [ ] All API keys present and valid
- [ ] No test keys in production config
- [ ] Key rotation schedule documented
- [ ] Backup keys secured
- [ ] HSM/vault access verified

---

## Pre-Release Checklist

**Duration**: ~30 minutes  
**Owner**: Release manager + QA

### Build Verification
- [ ] Production build succeeds
- [ ] Docker image builds successfully
- [ ] No build warnings or errors
- [ ] Artifact is signed/attested

### Testing Verification
- [ ] All tests pass in CI/CD
- [ ] Code coverage maintained
- [ ] Security scans pass
- [ ] Load test baseline met

### Documentation
- [ ] Changelog updated
- [ ] Security release notes prepared
- [ ] Deployment guide current
- [ ] Known issues documented

### Approval
- [ ] Security review approval
- [ ] DevOps approval
- [ ] Compliance approval (if needed)
- [ ] Release notes approved

---

## Incident Response Checklist

### Phase 1: Detection (5 min)

**Immediate Actions:**
- [ ] Alert received and acknowledged
- [ ] Incident severity assessed (P1-P4)
- [ ] On-call team notified
- [ ] Incident ticket created

**Assessment:**
- [ ] System still operational? (Y/N)
- [ ] User data affected? (Y/N)
- [ ] Authentication compromised? (Y/N)
- [ ] Data exfiltration risk? (Y/N)

### Phase 2: Containment (15 min)

**Stop the Bleeding:**
- [ ] Revoke compromised credentials
- [ ] Block suspicious IPs/accounts
- [ ] Rotate API keys immediately
- [ ] Isolate affected systems

**Preserve Evidence:**
- [ ] Backup logs before any cleanup
- [ ] Snapshot affected databases
- [ ] Archive firewall/IDS alerts
- [ ] Record system state

### Phase 3: Investigation (1-4 hours)

**Forensics:**
- [ ] Review access logs
- [ ] Check for unauthorized changes
- [ ] Identify entry point
- [ ] Determine attacker identity (if possible)

**Scope Assessment:**
- [ ] How many users affected?
- [ ] What data was accessed?
- [ ] How long was access available?
- [ ] Was data modified?

**Documentation:**
- [ ] Timeline of events
- [ ] Technical findings
- [ ] Evidence collected
- [ ] Preliminary root cause

### Phase 4: Remediation (2-8 hours)

**Fix the Vulnerability:**
- [ ] Patch deployed
- [ ] Fix verified in staging
- [ ] Production deployment completed
- [ ] Rollback plan ready

**Restore Trust:**
- [ ] Rotate all credentials
- [ ] Force password resets (if applicable)
- [ ] Reissue security tokens
- [ ] Verify no backdoors remain

### Phase 5: Communication (ongoing)

**Internal:**
- [ ] Executive team notified
- [ ] Customer support updated
- [ ] Legal/compliance alerted
- [ ] Team briefing completed

**External:**
- [ ] Customers notified (if data affected)
- [ ] Regulatory bodies notified (if required)
- [ ] Press statement prepared
- [ ] FAQ prepared for support

### Phase 6: Post-Incident (24+ hours)

**Learning:**
- [ ] Root cause analysis
- [ ] Contributing factors identified
- [ ] Detection gaps identified
- [ ] Response improvements noted

**Prevention:**
- [ ] Preventive measures implemented
- [ ] Monitoring enhanced
- [ ] Procedures updated
- [ ] Training conducted

---

## Daily Security Checks

**Time**: 10 minutes  
**Frequency**: Daily (beginning of shift)

```
08:00 - Daily Security Check
  [ ] Review overnight alerts
  [ ] Check error rates
  [ ] Verify backups completed
  [ ] Scan logs for anomalies
  [ ] Confirm all services healthy
  [ ] Check security monitoring active
```

### Critical Metrics to Review

**Authentication:**
- [ ] Failed login attempts < 10/hour
- [ ] Unique IPs per user < 5 in 24h
- [ ] No signature verification errors

**System Health:**
- [ ] API response time P99 < 1s
- [ ] Error rate < 0.1%
- [ ] Database replication lag < 1s
- [ ] Disk usage < 80%

**Security:**
- [ ] No security alerts
- [ ] Rate limiter active
- [ ] CORS validation working
- [ ] TLS cert valid > 30 days

---

## Weekly Security Review

**Duration**: 30 minutes  
**Frequency**: Weekly (Friday)

- [ ] Review incident log
- [ ] Check vulnerability scans
- [ ] Update threat board
- [ ] Review access logs for anomalies
- [ ] Verify backup integrity
- [ ] Check certificate expiration dates
- [ ] Review rate limiting effectiveness
- [ ] Test alert notification chain

---

## Monthly Security Tasks

**Duration**: 2-4 hours  
**Frequency**: Monthly (last Friday)

- [ ] Rotate API keys
- [ ] Update security dependencies
- [ ] Review and update runbooks
- [ ] Conduct security training topic
- [ ] Run security audit tools
- [ ] Review and update threat model
- [ ] Check compliance status
- [ ] Plan next month's security work

---

## Quarterly Security Assessment

**Duration**: Full day  
**Frequency**: Quarterly

### Assessment Items
- [ ] Full penetration testing
- [ ] Vulnerability scanning (active)
- [ ] Code security review (sampling)
- [ ] Access control audit
- [ ] Encryption key review
- [ ] Incident response drill
- [ ] Disaster recovery test
- [ ] Third-party dependency audit

### Outcomes
- [ ] Report generated
- [ ] Issues prioritized
- [ ] Remediation plan created
- [ ] Executive summary prepared

---

## Annual Security Audit

**Duration**: 2-3 weeks  
**Frequency**: Annually

### Full Audit Scope
- [ ] Complete code review
- [ ] Infrastructure assessment
- [ ] Security policy review
- [ ] Compliance verification
- [ ] Third-party security assessment
- [ ] Penetration test
- [ ] Social engineering test
- [ ] Disaster recovery full drill

### Deliverables
- [ ] Audit report
- [ ] Executive summary
- [ ] Risk register
- [ ] Remediation roadmap
- [ ] Board presentation

---

## Key Personnel Contacts

### Security Team
- **CISO**: [Name] [Email] [Phone]
- **Security Engineer**: [Name] [Email] [Phone]
- **Incident Commander**: [Name] [Email] [Phone]

### Infrastructure Team
- **DevOps Lead**: [Name] [Email] [Phone]
- **Database Admin**: [Name] [Email] [Phone]
- **On-Call**: [Escalation procedure]

### Executive
- **CTO**: [Name] [Email] [Phone]
- **CEO**: [Name] [Email] [Phone]

### External
- **Security Vendor**: [Company] [Phone] [Portal]
- **Incident Response Firm**: [Company] [Phone] [Contract #]
- **Legal Counsel**: [Company] [Email] [Phone]

---

## Emergency Contacts

### Security Incidents
- **24/7 Hotline**: [Number]
- **Email**: security-emergency@atomicip.dev
- **Slack**: #security-incidents

### System Down
- **DevOps On-Call**: [Number]
- **Slack**: #incidents
- **Status Page**: status.atomicip.dev

---

## Document Management

**Version**: 1.0  
**Last Updated**: 2024-09-24  
**Next Review**: 2025-01-24  
**Owner**: Security Team  
**Distribution**: Internal (Confidential)
