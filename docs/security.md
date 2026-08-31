# Security Considerations for IP Creators

This guide covers best practices for protecting your intellectual property when using AtomicIP.

## Secret Management

Your **secret** and **blinding factor** are the only proof of IP ownership. Losing them means losing the ability to verify, sell, or reveal your IP — permanently.

### Storage Rules

- **Never store secrets on-chain or in plaintext.** The blockchain is public; only the commitment hash goes on-chain.
- **Use encrypted storage** — a password manager (e.g., Bitwarden, 1Password) or an encrypted file vault.
- **Create at least two backups** stored in separate physical locations (e.g., encrypted USB + encrypted cloud).
- **Never share your secret** until you are ready to complete an atomic swap. Once revealed, ownership cannot be re-hidden.

### What to Store

For each IP commitment, securely store all three values together:

| Value | Description |
|---|---|
| `secret` | 32-byte hash of your IP document |
| `blinding_factor` | 32-byte random value |
| `ip_id` | The on-chain ID returned by `commit_ip` |

Losing any one of these makes it impossible to call `verify_commitment` or complete a swap.

### If Your Secret Is Compromised

If someone learns your secret before you reveal it:

- They **cannot** complete a swap — they need your Stellar wallet signature.
- They **cannot** transfer or revoke your IP — `require_auth()` enforces this at the protocol level.
- You should still be able to prove ownership via your on-chain timestamp and wallet signature.

Immediately revoke the IP record using `revoke_ip` and re-register with a new secret if you suspect compromise.

---

## Request Signing (API Authentication)

Every API request must be signed with your Stellar Ed25519 keypair to prove ownership of your address.

### Request Signature Scheme

The API server enforces request signing using Ed25519 (Stellar's standard). Signatures bind four pieces of data:

```
SignaturePayload = METHOD || SEPARATOR || PATH || SEPARATOR || TIMESTAMP || SEPARATOR || BODY_HASH
```

Where:
- `METHOD` is the HTTP verb (GET, POST, etc.)
- `PATH` is the request path (e.g., `/v1/ips`)
- `TIMESTAMP` is the current Unix timestamp in seconds
- `BODY_HASH` is the SHA-256 hash of the request body (or empty string for GET/DELETE)
- `SEPARATOR` is `||` (two pipe characters)

### Required Headers

Include these headers with every request:

| Header | Value | Example |
|---|---|---|
| `X-Address` | Your Stellar public key | `GCZXWVG5FGTWTJWY5M3DMX3S2Z4XYFABXJZLOWMVTQKJMHFJGDQGSVRQ` |
| `X-Timestamp` | Unix timestamp (seconds) | `1725062400` |
| `X-Signature` | Ed25519 signature (hex-encoded) | `a7f3e4c2b1d9f8e6...` |

### Signature Generation

1. Construct the signing payload:
   ```
   POST||/v1/swaps||1725062400||a7f3e4c2b1d9f8e6a5c3d2e1f0a9b8c7d6e5f4a3b2c1d0e9f8a7b6c5d4e3f2
   ```

2. Hash with SHA-256:
   ```
   message_hash = SHA256(signing_payload.encode())
   ```

3. Sign with your Ed25519 private key:
   ```
   signature = Ed25519_Sign(private_key, message_hash)
   signature_hex = hex_encode(signature.bytes())
   ```

### Example (Python)

```python
import ed25519
import hashlib
import time
from binascii import hexlify

# Your Stellar keypair
verify_key, signing_key = ed25519.create_keypair()  # Use your existing keypair
public_key = verify_key.encode()

method = "POST"
path = "/v1/swaps"
timestamp = int(time.time())
body = '{"ip_id": 1, "seller": "GCZXWVG...", ...}'
body_hash = hashlib.sha256(body.encode()).hexdigest()

# Construct signing payload
payload = f"{method}||{path}||{timestamp}||{body_hash}"
message_hash = hashlib.sha256(payload.encode()).digest()

# Sign
signature = signing_key.sign(message_hash)
signature_hex = hexlify(signature).decode()

# Make request with headers
headers = {
    "X-Address": public_key.decode(),
    "X-Timestamp": str(timestamp),
    "X-Signature": signature_hex
}
```

### Timestamp Validation

- The server rejects requests if `|server_time - X-Timestamp|` exceeds 300 seconds (configurable via `REQUEST_SIGNATURE_SKEW_SECS`)
- This prevents request replay attacks across different times or in different orders
- Ensure your client clock is synchronized with NTP

### Signature Replay Prevention

Signatures cannot be replayed because they bind:
- **HTTP method** — changing GET to POST invalidates the signature
- **Request path** — changing `/v1/ips` to `/v1/ips/1` invalidates the signature
- **Timestamp** — re-using the same signature at a later time is rejected as outside the skew window
- **Request body** — modifying any field in the JSON payload changes the body hash, invalidating the signature

---

## Rate Limiting

The API server enforces per-IP and per-user rate limits to prevent abuse and ensure fair resource allocation.

### Rate Limit Tiers

| Tier | Global Limit | Per-IP Limit | Per-User Limit | Burst |
|---|---|---|---|---|
| **Unauthenticated** | 1000 req/min | 30 req/min | N/A | 10 |
| **Free (authenticated)** | 1000 req/min | 60 req/min | 300 req/min | 20 |
| **Premium** | 1000 req/min | 200 req/min | 2000 req/min | 50 |
| **Enterprise** | 1000 req/min | 500 req/min | 10000 req/min | 200 |

Limits use a **token-bucket algorithm**:
- Tokens refill at a constant rate (e.g., `300 req/min` = 5 tokens/sec)
- Requests consume 1 token; burst capacity allows short spikes above the sustained rate
- When a bucket is empty, requests are rejected with HTTP 429 (Too Many Requests)

### Rate Limit Headers

Each response includes these headers to show your quota status:

| Header | Meaning |
|---|---|
| `RateLimit-Limit` | Requests allowed in the current window |
| `RateLimit-Remaining` | Requests remaining in the current window |
| `RateLimit-Reset` | Unix timestamp when the current window resets |

### Handling Rate Limits

When you receive a 429 response:

```bash
# Recommended backoff strategy
initial_backoff = 1  # second
max_backoff = 60     # seconds

# Read RateLimit-Reset to see when rate limit resets
retry_after = response.headers['Retry-After']  # Seconds to wait
```

### Multi-Instance Deployments

- **In-process mode** (default): Each API server instance enforces its own quota — N replicas means N times the documented quota
- **Redis mode** (`REDIS_URL=redis://...`): All replicas share one Redis instance; quota is global across the load balancer

---

## RPC Circuit Breaker

The API server protects itself and downstream services (Soroban RPC, price oracles, databases) using a circuit breaker that detects cascading failures.

### State Machine

```
           ┌─────────────────┐
           │     CLOSED      │◄──────────────────────────────────────────┐
           │  (normal flow)  │                                           │
           └────────┬────────┘                                           │
                    │ failure_threshold consecutive failures             │
                    ▼                                                     │
           ┌─────────────────┐   timeout_secs elapses   ┌───────────────┴──┐
           │      OPEN       │─────────────────────────►│    HALF-OPEN     │
           │ (rejects calls) │                           │  (test requests) │
           └─────────────────┘◄──────────────────────── └──────────────────┘
                                  any failure               success_threshold
                                                          consecutive successes
```

### Configuration

Default settings protect against brief outages while allowing recovery:

| Parameter | Default | Description |
|---|---|---|
| `failure_threshold` | `5` | Consecutive failures before opening the circuit |
| `success_threshold` | `2` | Consecutive successes in HalfOpen state to close |
| `timeout_secs` | `30` | Seconds the circuit stays Open before testing recovery |
| `half_open_max_calls` | `3` | Max concurrent test calls while Half-Open |

### Named Breakers

Each external service gets an independent breaker (isolation):

| Service | Purpose |
|---|---|
| `soroban-rpc` | Stellar Soroban RPC node calls |
| `price-oracle` | Price feed for swap valuation |
| `postgres` | PostgreSQL database |
| `redis` | Redis caching layer |

### Behavior

**Closed (Normal):**
- All requests pass through to the downstream service
- Failures are counted; `failure_threshold` consecutive failures open the circuit

**Open (Failing):**
- All new requests are rejected immediately with `CircuitOpen` error
- No calls reach the downstream service
- After `timeout_secs`, the circuit transitions to Half-Open to test recovery

**Half-Open (Testing Recovery):**
- Up to `half_open_max_calls` test requests are allowed through
- If `success_threshold` consecutive requests succeed, the circuit closes (recovery confirmed)
- Any failure re-opens the circuit immediately
- Requests beyond `half_open_max_calls` are rejected until the state changes

### Metrics (Prometheus)

All circuits emit these metrics on `GET /metrics`:

```promql
# State transitions (debugging cascades)
circuit_breaker_state_transitions_total{service="soroban-rpc", from="closed", to="open"}

# Current state (0=closed, 1=open, 2=half_open)
circuit_breaker_state{service="soroban-rpc"}

# Total calls ever attempted
circuit_breaker_calls_total{service="soroban-rpc"}

# Rejected calls (only when open)
circuit_breaker_calls_rejected_total{service="soroban-rpc"}
```

### Alerting

Set up alerts for circuit degradation:

```promql
# Circuit has been open for more than 60 seconds
circuit_breaker_state{service="soroban-rpc"} == 1

# More than 10% of requests rejected (high failure rate)
rate(circuit_breaker_calls_rejected_total[1m])
  / rate(circuit_breaker_calls_total[1m]) > 0.1

# Rapid state thrashing (unstable service)
rate(circuit_breaker_state_transitions_total{from="open",to="half_open"}[1m]) > 5
```

---

## Key Derivation Recommendations

### Deriving a Secret from Your IP Document

The recommended approach is to derive your secret deterministically from the actual IP content:

```
secret = sha256(your_design_document_bytes)
```

This ties the secret to the content — if you still have the document, you can always re-derive the secret.

**Example (off-chain, using any SHA-256 tool):**

```bash
# Linux/macOS
sha256sum my_design.pdf

# Or in Python
python3 -c "import hashlib, sys; print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())" my_design.pdf
```

### Generating a Blinding Factor

The blinding factor must be **cryptographically random** — never use predictable values like all-zeros or sequential numbers.

```bash
# Generate 32 random bytes (hex) on Linux/macOS
openssl rand -hex 32
```

Store the output alongside your secret. There is no way to recover a lost blinding factor.

### Key Derivation Anti-Patterns

| ❌ Don't Do This | Why |
|---|---|
| `blinding_factor = [0u8; 32]` | Trivially guessable; breaks hiding property |
| Reuse the same secret for multiple IPs | One leak exposes all linked IPs |
| Derive blinding factor from secret | Reduces entropy; both values must be independent |
| Store secret in the same location as your Stellar private key | Single point of failure |

---

## Commitment Hash Construction

The commitment hash registered on-chain is:

```
commitment_hash = sha256(secret || blinding_factor)
```

Both `secret` and `blinding_factor` must be exactly **32 bytes**. The concatenation is 64 bytes total before hashing.

Verify your commitment hash locally before submitting — once registered, it cannot be changed.

---

## Wallet Security

- Use a **dedicated Stellar wallet** for IP registration, separate from your main funds wallet.
- Enable **hardware wallet signing** if available.
- Never expose your Stellar private key. The `require_auth()` check in every contract function means your key is the final gate on all IP operations.

---

## Swap Security

Before accepting or initiating a swap:

- Verify the `ip_id` matches the IP you intend to sell/buy.
- Check the swap `expiry` — buyers can cancel after expiry if the seller has not revealed the key.
- Only interact with the verified AtomicSwap contract address from the official deployment.

See [atomic-swap.md](atomic-swap.md) for the full swap flow.

---

## Summary Checklist

- [ ] Secret derived from or tied to actual IP content
- [ ] Blinding factor generated with a CSPRNG
- [ ] Both values stored encrypted, with at least two backups
- [ ] Commitment hash verified locally before on-chain submission
- [ ] Dedicated Stellar wallet used for IP operations
- [ ] Secret never shared until swap `reveal_key` is called
