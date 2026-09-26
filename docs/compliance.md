# Compliance and Legal Guidance

This document is an implementation guide for teams operating AtomicIP. It is
not legal advice. The operator remains responsible for identifying the laws
that apply to its users, jurisdictions, and deployment model and for obtaining
professional advice where required.

## Regulatory requirements

Before launching a production integration, document:

- the jurisdictions in which the operator, users, and service providers are
  located;
- whether the service is subject to registration, licensing, sanctions
  screening, anti-money-laundering, or know-your-customer requirements;
- which party is responsible for custody, payments, dispute handling, and
  intellectual-property claims; and
- retention, disclosure, incident-response, and regulator-contact obligations.

Smart contracts do not remove obligations that apply to the surrounding
application, wallet, API, or operator.

## Data privacy

AtomicIP commitments are designed to prove possession of data without
publishing the underlying work. Operators should still treat addresses,
metadata, API logs, audit records, and support requests as potentially
personal data.

1. Define a data inventory and lawful purpose for every off-chain field.
2. Minimize collection and do not put personal data or secrets in commitment
   payloads, transaction metadata, or public logs.
3. Publish a privacy notice covering collection, sharing, retention, rights
   requests, and cross-border transfers.
4. Encrypt sensitive off-chain data, restrict access, and establish deletion or
   anonymization procedures. On-chain data may be immutable and should not be
   used as the sole store for information that must later be erased.
5. Record processor/subprocessor relationships and assess breach-notification
   requirements before production use.

## Terms of service

The operator's terms should identify the service provider and acceptable use
rules, explain that blockchain transactions are public and generally
irreversible, define fees and support boundaries, and describe intellectual
property ownership, licensing, disputes, sanctions controls, service
availability, liability limits, and termination. Obtain affirmative acceptance
where required and retain the version accepted by each user.

## Audit trail procedures

Retain an append-only record of administrative actions, deployments,
configuration changes, access grants, ownership changes, disputes, and
incident-response decisions. Each entry should include the actor, timestamp,
request or transaction identifier, action, affected resource, result, and
relevant contract/network identifier. Protect audit records from alteration,
restrict access, synchronize clocks, and test restoration regularly.

Do not log private keys, credentials, raw personal data, or sensitive
commitment inputs. Use the redaction and retention controls documented in
[`docs/security.md`](security.md) and the API's audit facilities.

## Production checklist

- [ ] Jurisdictions, regulatory classifications, and responsible owners are
      documented.
- [ ] Privacy notice, data inventory, retention schedule, and rights-request
      process are approved.
- [ ] Terms of service and acceptable-use policy are published and versioned.
- [ ] Sanctions/AML/KYC controls are assessed and enabled where applicable.
- [ ] Audit events, access controls, redaction, retention, and restoration
      have been tested.
- [ ] Incident contacts, breach response, and regulator notifications are
      documented.
- [ ] A contract/network upgrade and rollback owner is on call.

Review this checklist after material product, jurisdiction, contract, or
provider changes.
