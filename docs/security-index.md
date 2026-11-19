# Security Documentation Index

Security guidance is split across several files. This index states the role of
each one. When two documents disagree, the file marked **Authoritative** wins.

| File | Audience | Role |
|------|----------|------|
| [`../SECURITY.md`](../SECURITY.md) | Reporters and everyone | **Authoritative for vulnerability reporting**, disclosure policy, known limitations and compliance scope. |
| [`SECURITY_HARDENING.md`](SECURITY_HARDENING.md) | Operators and developers | **Authoritative hardening guide**: best practices, deployment checklists, key management, common vulnerabilities and operational checklists (Part II). |
| [`security.md`](security.md) | IP creators using the API | Practical guidance for protecting secrets, request signing, rate limits and the RPC circuit breaker. |
| [`SECURITY_CHECKLIST.md`](SECURITY_CHECKLIST.md) | — | Redirect only. Its content moved to Part II of `SECURITY_HARDENING.md`. |
| [`threat-model.md`](threat-model.md) | Reviewers | Threats, assets and mitigations. Referenced by the hardening guide. |
| [`incident-response.md`](incident-response.md) | On-call responders | Step-by-step incident handling. |

## Rules

- Put each security requirement in exactly one authoritative file and link to it from elsewhere.
- Keep the checklists in `SECURITY_HARDENING.md` Part II, not in separate files.
- Update this index in the same PR when a security document is added, removed or renamed.
