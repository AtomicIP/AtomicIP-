# Recovering a Lost Commitment

An AtomicIP commitment is recoverable only when you can recover the original
`ip_id`, `secret`, and `blinding_factor`. The blockchain stores the commitment
hash, not the secret or blinding factor. Neither the API nor the Stellar
network can reconstruct missing private values.

## 1. Find the on-chain record

Use the wallet address that created the commitment:

```sh
curl "https://api.example.com/v1/ip/owner/GOWNER..."
```

For a large account, follow the cursor returned by
`/v1/ip/owner/{owner}/cursor` until the target `ip_id` is found. Record the
`ip_id` and confirm the owner and timestamp from the on-chain record.

## 2. Restore the private values

Check, in order:

1. Your encrypted password manager entry.
2. An offline encrypted backup.
3. The encrypted project archive used when the commitment was created.
4. The wallet or device backup that stores the commitment export.

Do not email or paste the secret or blinding factor into a support ticket.

## 3. Verify before using the commitment

Compute `sha256(secret || blinding_factor)` locally and compare it with the
record's `commitment_hash`. Only send the values to the verification endpoint
after the local hash matches:

```sh
curl -X POST "https://api.example.com/v1/ip/verify" \
  -H "Content-Type: application/json" \
  -d '{"ip_id":123,"secret":"<hex>","blinding_factor":"<hex>"}'
```

A successful verification restores the ability to reveal the key or use the
commitment in an atomic swap. Keep the recovered values encrypted and create a
second offline backup immediately.

## If recovery fails

If the secret or blinding factor is permanently lost, the commitment cannot be
verified or transferred. Do not register a replacement commitment with the
same private values. Create a new secret and blinding factor, register a new
commitment, and retain the new recovery material in two encrypted locations.

If the values may have been exposed, treat them as compromised: do not reuse
them, revoke the affected IP when possible, and create a new commitment. A
Stellar wallet signature is still required for protocol-level ownership
operations, but exposed commitment material should not be trusted.
