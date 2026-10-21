# Notary Signature Message Format

Status: **version 1** (current). Introduced by #1149.

The IP Registry's `notarize_ip_timestamp` verifies an Ed25519 signature made
with the notary key configured by `set_notary_public_key`. The signed message
is built by `notary_signing_message` in `contracts/ip_registry/src/lib.rs`.

## Why a domain tag

The earlier format signed only `ip_id || timestamp`. Any notary key shared
between two deployments let a signature issued for one deployment be replayed
on the other. The v1 message binds each signature to one deployment (contract
address) on one network, and to a domain tag that marks it as a notary
signature.

## Layout

All fields are concatenated in this order:

| # | Field | Size | Encoding |
|---|-------|------|----------|
| 1 | Version byte | 1 byte | `0x01` |
| 2 | Domain tag | 15 bytes | ASCII `AtomicIP/notary` |
| 3 | Network id | 32 bytes | `SHA-256(network passphrase)`, as returned by `Ledger::network_id` |
| 4 | Contract address | 40 bytes | XDR encoding of the `Address` (`ScAddress`) of the IP Registry deployment |
| 5 | `ip_id` | 8 bytes | big-endian `u64` |
| 6 | `timestamp` | 8 bytes | big-endian `u64`, the IP record's `timestamp` |

The signature is a 64-byte Ed25519 signature over the full message. The
contract checks it with `ed25519_verify` against the stored notary public key.

## Version history

| Version | Message | Status |
|---------|---------|--------|
| 0 (implicit) | `ip_id \|\| timestamp` (16 bytes, no tag) | **Rejected** since v1 |
| 1 | As in the table above | Current |

Version 0 signatures never verify under v1. Notaries must re-sign every IP
they want to notarize against a v1 deployment.

## Rules for implementers

- Build the message with the same field order and encodings. Do not reuse a
  signature across deployments or networks.
- To add a new format, increment the version byte and document the new layout
  here. Do not change the layout of an existing version.

## Test coverage

`contracts/ip_registry/src/notary_access_tests.rs` covers:

- a v1 signature from the correct deployment is accepted;
- a v1 signature issued for a different deployment is rejected;
- a legacy (version 0) signature is rejected;
- a second notarization for the same IP is rejected (see #1148).
