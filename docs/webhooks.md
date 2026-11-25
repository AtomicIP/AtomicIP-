# Webhook Verification Guide

AtomicIP signs every outgoing webhook HTTP `POST` request with a per-webhook secret using HMAC-SHA256 so receivers can verify both authenticity and freshness.

## Webhook Secret

When you register a webhook via `POST /webhooks` (or `POST /v1/webhooks`), the response includes a `secret` field (a 32-byte random hex-encoded secret):

```json
{
  "id": "wh_...",
  "url": "https://example.com/webhooks/atomicip",
  "events": ["swap.status_changed"],
  "created_at": 1700000000,
  "secret": "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
}
```

**Store this secret securely.** It is returned **once** on registration and is not included in subsequent `GET /webhooks` responses.

## Request Headers

Every webhook delivery includes the following headers:

| Header | Description |
| --- | --- |
| `X-Webhook-Signature` | Lowercase hex-encoded HMAC-SHA256 digest of `"{timestamp}.{body}"` |
| `X-Webhook-Timestamp` | Unix timestamp (seconds) when the signature was generated |
| `X-Webhook-Id` | Unique delivery UUID for idempotency |

## Verification Steps

1. Read `X-Webhook-Timestamp` and `X-Webhook-Signature` from the request headers, and read the **raw** request body bytes as UTF-8.
2. Parse `X-Webhook-Timestamp` as an integer and reject requests outside your replay window (recommended: `|now - timestamp| <= 300` seconds).
3. Construct the signed payload string:
   ```text
   {timestamp}.{raw_body}
   ```
4. Compute `HMAC-SHA256(secret, signed_payload)` and encode the result as lowercase hex.
5. Compare the computed digest with `X-Webhook-Signature` using a **constant-time** comparison function.

### Example (Node.js)

```js
const crypto = require("crypto");

function verifyWebhook(secret, timestampHeader, signatureHeader, rawBody, maxSkewSecs = 300) {
  const ts = Number(timestampHeader);
  if (!Number.isInteger(ts)) return false;

  const now = Math.floor(Date.now() / 1000);
  if (Math.abs(now - ts) > maxSkewSecs) return false;

  const expected = crypto
    .createHmac("sha256", secret)
    .update(`${ts}.${rawBody}`)
    .digest("hex");

  const sigBuf = Buffer.from(signatureHeader, "utf8");
  const expBuf = Buffer.from(expected, "utf8");
  if (sigBuf.length !== expBuf.length) return false;

  return crypto.timingSafeEqual(sigBuf, expBuf);
}
```

## Test Vector

Use the following known-answer test vector to validate your verifier implementation:

- **Secret:** `whsec_test_secret_0123456789abcdef`
- **Timestamp (`X-Webhook-Timestamp`):** `1700000000`
- **Raw Body:**
  ```json
  {"event":"swap.status_changed","swap_id":42,"status":"Completed","timestamp":1700000000}
  ```
- **Signed Input (`"{timestamp}.{body}"`):**
  ```text
  1700000000.{"event":"swap.status_changed","swap_id":42,"status":"Completed","timestamp":1700000000}
  ```
- **Expected `X-Webhook-Signature` (HMAC-SHA256 hex):**
  ```text
  51866deec8c2a53c16a8d08bf3e1f6e50134f4674a87e03cc829a087e9e0854a
  ```
