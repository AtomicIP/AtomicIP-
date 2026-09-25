# Security Hardening Guide for AtomicIP

This guide provides comprehensive security best practices, deployment checklists, and hardening strategies for securing AtomicIP systems in production environments.

**Document Version:** 1.0  
**Last Updated:** 2024-09-24  
**Classification:** Public

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Security Best Practices](#security-best-practices)
3. [Deployment Security Checklist](#deployment-security-checklist)
4. [Key Management Guide](#key-management-guide)
5. [Common Vulnerabilities](#common-vulnerabilities)
6. [Mitigation Strategies](#mitigation-strategies)
7. [Monitoring and Response](#monitoring-and-response)
8. [Security Incident Response](#security-incident-response)

---

## Executive Summary

AtomicIP is a decentralized intellectual property registry built on Stellar Soroban. Security is critical because the system handles valuable digital assets and requires cryptographic proof of ownership.

This guide addresses:
- **Authentication**: Request signing and keypair management
- **Authorization**: Access control and privilege separation
- **Confidentiality**: Secret protection and encryption
- **Integrity**: Data validation and cryptographic verification
- **Availability**: Rate limiting and DDoS protection

---

## Security Best Practices

### 1. Cryptographic Key Management

#### Private Key Protection
```
✓ DO:
  - Store private keys in encrypted vaults (HSM, AWS KMS, HashiCorp Vault)
  - Use strong encryption (AES-256) with unique passphrases
  - Implement key rotation every 90 days
  - Use hardware security modules (HSM) for production keys
  - Restrict access to private keys using principle of least privilege

✗ DON'T:
  - Store private keys in plaintext
  - Hardcode keys in source code or configuration files
  - Use weak passphrases or default keys
  - Share keys across environments
  - Log private keys in any form
```

#### Keypair Generation
```javascript
// ✓ Correct: Use cryptographically secure RNG
const crypto = require('crypto');
const privateKey = crypto.randomBytes(32);

// ✗ Wrong: Using non-secure random
const privateKey = Math.random().toString(36).substring(2);
```

### 2. Request Authentication

All API requests must be signed with Ed25519 keypairs to prevent authentication bypass.

**Required Headers:**
```
X-Address: Your Stellar public key
X-Timestamp: Current Unix timestamp (seconds)
X-Signature: Ed25519 signature (hex-encoded)
```

**Signature Scheme:**
```
message = METHOD || PATH || TIMESTAMP || BODY_HASH
signature = Ed25519_Sign(private_key, SHA256(message))
```

**Timestamp Validation:**
- Accept timestamps within ±5 minutes of server time
- Reject expired timestamps to prevent replay attacks
- Return 401 Unauthorized for invalid signatures

### 3. Input Validation

Validate all user input at system boundaries:

```javascript
// ✓ Validate amounts
function validateAmount(amount) {
  const num = parseFloat(amount);
  if (isNaN(num)) return false;
  if (num <= 0) return false;
  if (num > 10**15) return false; // Prevent overflow
  if (amount.split('.')[1]?.length > 7) return false; // Max 7 decimals
  return true;
}

// ✓ Validate IP hashes (hex, exact length)
function validateIPHash(hash) {
  return /^[a-f0-9]{64}$/.test(hash); // 32 bytes = 64 hex chars
}

// ✓ Validate addresses
function validateStellarAddress(address) {
  return /^G[A-Z2-7]{55}$/.test(address);
}

// ✓ Validate paths
function validatePath(path) {
  return /^\/[a-zA-Z0-9\/_\-\.]*$/.test(path);
}
```

### 4. Secret Protection

Secrets (IP hashes and blinding factors) are cryptographic commitments, not passwords:

```javascript
// ✓ Hash secrets before any storage or transmission
const secret = crypto.randomBytes(32);
const secretHash = crypto.createHash('sha256').update(secret).digest('hex');

// ✓ Never log secrets
console.log('IP committed'); // ✓ Good
console.log(`Secret: ${secret}`); // ✗ Bad

// ✓ Clear sensitive data from memory
function clearSensitiveData(data) {
  if (data && typeof data === 'object') {
    for (const key in data) {
      if (typeof data[key] === 'string') {
        data[key] = '';
      }
    }
  }
}
```

### 5. Access Control

Implement principle of least privilege (PoLP):

```javascript
// ✓ User can only access their own records
function canAccessIPRecord(userId, recordOwnerId) {
  return userId === recordOwnerId;
}

// ✓ Admin operations require explicit token
function requireAdminToken(user) {
  if (user.role !== 'admin' || !user.adminToken) {
    throw new Error('Insufficient permissions');
  }
}

// ✓ Verify ownership before modifications
function validateModifyPermission(userId, record) {
  if (userId !== record.owner) {
    throw new Error('Not authorized to modify this record');
  }
}
```

### 6. Secure Communications

**HTTPS/TLS:**
- Always use TLS 1.3 or higher in production
- Use certificates signed by trusted CAs
- Implement certificate pinning for critical connections
- Disable older protocols (SSL 3.0, TLS 1.0, TLS 1.1)

**Certificate Configuration:**
```
Minimum: TLS 1.3
Cipher Suites: ECDHE_RSA_AES_256_GCM_SHA384, ECDHE_RSA_CHACHA20_POLY1305
Perfect Forward Secrecy: Required
```

### 7. Rate Limiting

Protect against brute force and DoS attacks:

```javascript
const RateLimiter = require('express-rate-limit');

// Signing endpoint (high security)
const signLimiter = RateLimiter({
  windowMs: 60 * 1000, // 1 minute
  max: 100,            // 100 requests per minute
  message: 'Too many signing attempts, please try again later',
});

// General API (moderate security)
const apiLimiter = RateLimiter({
  windowMs: 15 * 60 * 1000, // 15 minutes
  max: 1000,                // 1000 requests per 15 minutes
});

app.post('/v1/sign', signLimiter, signHandler);
app.use('/v1/', apiLimiter);
```

### 8. Error Handling

Never expose internal details in error messages:

```javascript
// ✓ Good: Generic message to user
catch (error) {
  logger.error(`Database error: ${error.message}`, { userId, timestamp: Date.now() });
  res.status(500).json({ error: 'An error occurred. Please try again later.' });
}

// ✗ Bad: Exposing internal paths and details
catch (error) {
  res.status(500).json({
    error: `Connection to /var/db/postgres failed: ${error.toString()}`
  });
}
```

---

## Deployment Security Checklist

Use this checklist when deploying AtomicIP to production:

### Pre-Deployment

- [ ] **Code Review**: All code reviewed by security team
- [ ] **Dependency Audit**: `npm audit` passes with no critical vulnerabilities
- [ ] **SAST Scan**: Static analysis completed (eslint, snyk)
- [ ] **Secret Scan**: No secrets committed to repository
- [ ] **Build Test**: Build succeeds in clean environment
- [ ] **Test Coverage**: Minimum 70% code coverage maintained
- [ ] **Performance Tests**: Load tests pass with <500ms avg response time

### Network Security

- [ ] **HTTPS Only**: All endpoints require TLS 1.3+
- [ ] **Certificate Valid**: SSL certificate valid and not self-signed
- [ ] **HSTS Enabled**: Strict-Transport-Security header set
- [ ] **Firewall Rules**: Restrict access to admin endpoints
- [ ] **DDoS Protection**: CloudFlare or similar enabled
- [ ] **VPN Access**: Admin interfaces behind VPN only
- [ ] **WAF Rules**: Web Application Firewall configured

### Authentication & Authorization

- [ ] **API Keys Rotated**: All API keys recent (< 90 days old)
- [ ] **Service Accounts**: Non-production credentials revoked
- [ ] **MFA Enabled**: Multi-factor authentication on admin accounts
- [ ] **RBAC Configured**: Role-based access control properly configured
- [ ] **Default Passwords Changed**: All default credentials updated
- [ ] **OAuth2 Configured**: If using external auth, properly configured

### Data Protection

- [ ] **Encryption at Rest**: Database encryption enabled (AES-256)
- [ ] **Encryption in Transit**: All data in transit encrypted (TLS 1.3)
- [ ] **Key Rotation**: Encryption keys rotated monthly
- [ ] **Backups Encrypted**: Database backups encrypted
- [ ] **Backup Access**: Backup access restricted and logged
- [ ] **Data Retention**: Implement proper data retention policies
- [ ] **PII Protection**: If applicable, PII encrypted and limited access

### Monitoring & Logging

- [ ] **Centralized Logging**: All logs aggregated in central system
- [ ] **Log Retention**: 90-day minimum retention policy
- [ ] **Monitoring Active**: Real-time monitoring on critical systems
- [ ] **Alerting Configured**: Alerts for suspicious activity
- [ ] **Audit Logging**: All admin actions logged
- [ ] **Performance Monitoring**: Response time and error rate monitoring
- [ ] **Security Monitoring**: Intrusion detection active

### Infrastructure

- [ ] **OS Patched**: Operating system fully patched
- [ ] **Dependencies Updated**: All dependencies current
- [ ] **Security Updates**: Automatic security updates enabled
- [ ] **Container Security**: If using containers, images scanned
- [ ] **Database Hardened**: Database server hardened per standards
- [ ] **Backup Tested**: Restore from backup tested successfully
- [ ] **Disaster Recovery**: DR plan documented and tested

### Compliance

- [ ] **Security Policy**: Security policies documented
- [ ] **Incident Plan**: Incident response plan documented
- [ ] **Legal Review**: Terms of service and privacy policy reviewed
- [ ] **Compliance Check**: Verify GDPR/CCPA compliance if applicable
- [ ] **Documentation**: Security documentation up to date

---

## Key Management Guide

### Stellar Keypair Management

#### Keypair Generation
```javascript
const StellarSDK = require('stellar-sdk');

// Generate new keypair
const keypair = StellarSDK.Keypair.random();
const publicKey = keypair.publicKey();    // G...
const secretKey = keypair.secret();       // S...

// Never transmit or log the secret key
console.log(`Public: ${publicKey}`);      // ✓ OK
// console.log(`Secret: ${secretKey}`);   // ✗ NEVER DO THIS
```

#### Storage Strategy

**Development Environment:**
```
Store in: .env file (git-ignored)
Encryption: Optional (for local development only)
Access: Developer only
Rotation: Not required
```

**Staging Environment:**
```
Store in: AWS Secrets Manager / HashiCorp Vault
Encryption: Required (AES-256)
Access: CI/CD pipeline + ops team
Rotation: 30 days
```

**Production Environment:**
```
Store in: AWS CloudHSM / Hardware Security Module
Encryption: Hardware-backed
Access: Service account only (no human access)
Rotation: 14 days
Audit: All access logged and monitored
```

#### Key Rotation Procedure

```bash
#!/bin/bash
# Key rotation script (run monthly)

# Step 1: Generate new keypair
NEW_KEY=$(stellar-key-gen)

# Step 2: Store securely
aws secretsmanager create-secret \
  --name /atomicip/keys/$(date +%Y%m%d) \
  --secret-string "$NEW_KEY"

# Step 3: Update service to use new key
# (Deploy with new SECRET_KEY environment variable)

# Step 4: Monitor old key for 30 days
# (Ensure all pending operations complete)

# Step 5: Deactivate old key
aws secretsmanager retire-secret \
  --secret-id /atomicip/keys/old_date
```

#### Key Compromise Procedures

If a key is compromised:

1. **Immediately**:
   - Stop using the compromised key
   - Revoke all associated IP registrations
   - Alert affected users

2. **Within 1 hour**:
   - Generate new keypair
   - Deploy with new key
   - Update all services

3. **Investigation**:
   - Review access logs
   - Determine scope of compromise
   - Check for unauthorized operations

4. **Communication**:
   - Notify users of security incident
   - Provide instructions for secure recovery
   - Document lessons learned

---

## Common Vulnerabilities

### 1. Authentication Bypass

**Vulnerability**: Requests accepted without valid signature

**Indicators**:
```javascript
// ✗ VULNERABLE: Signature not checked
app.post('/v1/swaps', (req, res) => {
  const userId = req.headers['x-address'];
  // Process swap without verifying signature
  completeSwap(userId);
});

// ✓ SECURE: Signature verified
app.post('/v1/swaps', validateSignature, (req, res) => {
  const userId = req.headers['x-address'];
  completeSwap(userId);
});
```

**Impact**: CRITICAL - Attackers can impersonate users

### 2. Information Disclosure

**Vulnerability**: Sensitive data in error messages or logs

**Indicators**:
```javascript
// ✗ VULNERABLE: Error exposes internals
catch (error) {
  res.json({ error: error.stack }); // Stack trace exposed
}

// ✓ SECURE: Generic error message
catch (error) {
  logger.error(error); // Logged securely
  res.status(500).json({ error: 'Internal error' });
}
```

**Impact**: HIGH - Attackers learn system architecture

### 3. Privilege Escalation

**Vulnerability**: Regular users access admin functions

**Indicators**:
```javascript
// ✗ VULNERABLE: No permission check
app.delete('/v1/ips/:id', (req, res) => {
  deleteIP(req.params.id); // Any user can delete
});

// ✓ SECURE: Admin check required
app.delete('/v1/ips/:id', requireAdmin, (req, res) => {
  deleteIP(req.params.id);
});
```

**Impact**: CRITICAL - Attackers gain admin access

### 4. Data Exposure

**Vulnerability**: Confidential data in plaintext

**Indicators**:
- Secrets logged to console/files
- Unencrypted database backups
- Private keys in version control
- Credentials in CI/CD logs

**Impact**: CRITICAL - Loss of all security

### 5. Brute Force Attack

**Vulnerability**: No rate limiting on sensitive endpoints

**Indicators**:
```
POST /v1/sign - 10,000 requests in 1 minute
POST /v1/verify - 5,000 requests in 1 minute
```

**Impact**: HIGH - Accounts compromised through credential guessing

### 6. Replay Attack

**Vulnerability**: Old signatures accepted again

**Indicators**:
- Same signature accepted twice
- No timestamp validation
- No nonce mechanism

**Impact**: MEDIUM - Transactions can be replayed

---

## Mitigation Strategies

### Strategy 1: Defense in Depth

Implement multiple layers of security:

```
Layer 1: Network    - Firewall, TLS, DDoS protection
Layer 2: Auth       - Signature verification, MFA
Layer 3: App        - Input validation, rate limiting
Layer 4: Data       - Encryption, access control
Layer 5: Monitoring - Logging, alerting, forensics
```

### Strategy 2: Assume Breach

Plan for potential compromises:

```
1. Incident Response Team
   - Security engineer on-call 24/7
   - Clear escalation procedures
   - Regular incident drills

2. Detection Capabilities
   - Real-time monitoring of suspicious activity
   - Anomaly detection for transaction patterns
   - Automated alerting for threshold breaches

3. Recovery Procedures
   - Automated failover to backup systems
   - Key rotation in <1 hour
   - User notification templates
```

### Strategy 3: Security Posture Improvement

Continuous security enhancement:

```
Monthly:
  - Run security audit scripts
  - Review access logs for anomalies
  - Update dependencies
  - Rotate encryption keys

Quarterly:
  - Penetration testing
  - Security training for team
  - Update threat model
  - Review incident response procedures

Annually:
  - Full security assessment
  - Third-party audit
  - Update security policies
  - Disaster recovery drill
```

---

## Monitoring and Response

### Real-Time Monitoring

**Critical Metrics to Monitor:**

```javascript
// 1. Authentication Failures
app.post('/v1/sign', (req, res) => {
  if (!isValidSignature(req)) {
    metrics.counter('auth.failure', { reason: 'invalid_signature' });
    if (metrics.getTotalFailures() > 100) {
      alert('CRITICAL: High auth failure rate');
    }
  }
});

// 2. Unusual Access Patterns
function detectAnomalies(request) {
  const userId = request.userId;
  const recentActivity = getUserActivity(userId, '1h');
  
  if (recentActivity.count > 1000) {
    alert(`ALERT: High activity from ${userId}: ${recentActivity.count} requests`);
  }
  
  if (recentActivity.uniqueIPs > 10) {
    alert(`ALERT: Login from multiple IPs for ${userId}`);
  }
}

// 3. Error Rate Spike
function monitorErrorRate() {
  const errorRate = getErrorRate('5m');
  if (errorRate > 0.05) { // >5%
    alert(`CRITICAL: Error rate spike: ${errorRate}%`);
  }
}
```

**Alert Thresholds:**

| Metric | Warning | Critical |
|--------|---------|----------|
| Auth Failure Rate | >10/min | >100/min |
| API Error Rate | >1% | >5% |
| Response Time (P99) | >1s | >5s |
| Database Query Time | >500ms | >5s |
| Disk Usage | >80% | >95% |
| Memory Usage | >80% | >95% |

### Logging Best Practices

```javascript
// ✓ Log security-relevant events
logger.warn(`Unauthorized access attempt from ${ip}`, {
  userId,
  endpoint,
  timestamp,
  signature: 'invalid'
});

// ✗ Don't log sensitive data
logger.info(`User ${userId} action`, {
  privateKey: user.privateKey,  // ✗ NEVER
  apiToken: secret,              // ✗ NEVER
});

// ✓ Log with structured data
logger.info('Swap completed', {
  swapId,
  initiator,
  recipient,
  amount,
  timestamp: new Date().toISOString(),
  duration: `${endTime - startTime}ms`,
});
```

### Incident Response

**Incident Severity Levels:**

| Level | Response Time | Examples |
|-------|---------------|----------|
| P1 (Critical) | 15 minutes | Authentication bypass, data breach, DDoS |
| P2 (High) | 1 hour | Privilege escalation, unauthorized access |
| P3 (Medium) | 4 hours | Information disclosure, brute force |
| P4 (Low) | 24 hours | Configuration issues, non-critical bugs |

**Response Procedure:**

```
1. DETECT (automated)
   - Alert triggered
   - Incident ticket created
   - Team notified

2. RESPOND (manual)
   - Acknowledge alert
   - Gather initial information
   - Assess severity

3. CONTAIN (immediate)
   - Stop bleeding (block attacker)
   - Preserve evidence
   - Notify affected users

4. INVESTIGATE (hours)
   - Root cause analysis
   - Scope of compromise
   - Timeline of events

5. REMEDIATE (immediate)
   - Patch vulnerability
   - Deploy fix
   - Verify effectiveness

6. COMMUNICATE (ongoing)
   - User notifications
   - Regulatory reporting
   - Public statement

7. LEARN (post-incident)
   - Post-mortem report
   - Process improvements
   - Additional controls
```

---

## Additional Resources

### External References

- [NIST Cybersecurity Framework](https://www.nist.gov/cyberframework)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [Stellar Security Guide](https://developers.stellar.org/docs/glossary/authentication)
- [Ed25519 Best Practices](https://ed25519.cr.yp.to/)

### Internal References

- [docs/security.md](./security.md) - IP Creator Security Guide
- [docs/threat-model.md](./threat-model.md) - Threat Analysis
- [SECURITY.md](../SECURITY.md) - Security Policy

### Contact

For security issues:
- **Security Report**: security@atomicip.dev
- **Urgent Issues**: security-emergency@atomicip.dev
- **Public Key**: [GPG key for encrypted reports]

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2024-09-24 | Initial release |

---

**Last Review**: 2024-09-24  
**Next Review**: 2025-01-24
