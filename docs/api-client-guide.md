# API Client Guide

This guide shows how to interact with the Atomic Patent API from client applications. It covers authentication, error handling, retry logic, rate limiting, and common use cases with examples in TypeScript, Python, and Rust.

---

## Table of Contents

1. [Overview](#overview)
2. [Authentication](#authentication)
3. [Rate Limiting](#rate-limiting)
4. [Error Handling](#error-handling)
5. [Retry Logic](#retry-logic)
6. [Common Use Cases](#common-use-cases)
   - Commit IP
   - Initiate Swap
   - Accept Swap
   - Reveal Key
7. [Best Practices](#best-practices)

---

## Overview

The Atomic Patent API exposes REST endpoints for the IP Registry and Atomic Swap smart contracts. All endpoints expect and return JSON (`Content-Type: application/json`).

| Item | Value |
|------|-------|
| Base URL | `http://localhost:3000` (default local) |
| OpenAPI docs | `/docs` (Swagger UI) or `/openapi.json` |
| Protocol | HTTP/1.1 or HTTP/2 |
| Content-Type | `application/json` required for POST/PUT bodies |

### Quick Start

```bash
curl -X POST http://localhost:3000/ip/commit \
  -H "Content-Type: application/json" \
  -d '{"owner":"GABC...","commitment_hash":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}'
```

---

## Authentication

The API uses JWT bearer tokens. A Stellar Ed25519 keypair is used to prove identity: the client signs a challenge message, and the server returns an access token (15-minute expiry) and a refresh token (7-day expiry).

### Authentication Flow

1. **Sign a challenge** with your Stellar secret key.
2. **Send the public key, message, and signature** to the token endpoint.
3. **Receive `access_token` and `refresh_token`.**
4. **Include `Authorization: Bearer <access_token>`** on every subsequent request.

### Token Refresh

When the access token expires, use the refresh token to obtain a new access token without re-signing.

### TypeScript Example

```typescript
import { Keypair } from "@stellar/stellar-sdk";

async function authenticate(secretKey: string): Promise<{ access: string; refresh: string }> {
  const keypair = Keypair.fromSecret(secretKey);
  const publicKey = keypair.publicKey();
  const message = `auth:${Date.now()}`;
  const signature = keypair.sign(Buffer.from(message)).toString("hex");

  const res = await fetch("http://localhost:3000/auth/token", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ public_key: publicKey, message, signature }),
  });

  if (!res.ok) throw new Error(`Auth failed: ${res.status}`);
  return res.json();
}

async function apiRequest(path: string, token: string, body?: object) {
  const res = await fetch(`http://localhost:3000${path}`, {
    method: body ? "POST" : "GET",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`);
  return res.json();
}
```

### Python Example

```python
import requests
from stellar_sdk import Keypair

def authenticate(secret_key: str) -> dict:
    keypair = Keypair.from_secret(secret_key)
    public_key = keypair.public_key
    message = f"auth:{int(time.time() * 1000)}"
    signature = keypair.sign(message.encode()).hex()

    resp = requests.post(
        "http://localhost:3000/auth/token",
        json={"public_key": public_key, "message": message, "signature": signature},
    )
    resp.raise_for_status()
    return resp.json()

def api_request(path: str, token: str, json_body: dict = None):
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {token}",
    }
    method = requests.post if json_body else requests.get
    url = f"http://localhost:3000{path}"
    resp = method(url, headers=headers, json=json_body)
    resp.raise_for_status()
    return resp.json()
```

### Rust Example

```rust
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use stellar_strkey::Strkey;
use ed25519_dalek::{Signer, SigningKey};

async fn authenticate(secret_key: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    let signing_key = SigningKey::from_bytes(&hex::decode(secret_key)?);
    let public_key = Strkey::PublicKeyEd25519(stellar_strkey::ed25519::PublicKey(signing_key.verifying_key().to_bytes()));
    let message = format!("auth:{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis());
    let signature = hex::encode(signing_key.sign(message.as_bytes()));

    let client = reqwest::Client::new();
    let res = client
        .post("http://localhost:3000/auth/token")
        .json(&serde_json::json!({
            "public_key": public_key.to_string(),
            "message": message,
            "signature": signature,
        }))
        .send()
        .await?;

    let json: serde_json::Value = res.error_for_status()?.json().await?;
    Ok((json["access_token"].as_str().unwrap().to_string(), json["refresh_token"].as_str().unwrap().to_string()))
}

async fn api_request(path: &str, token: &str, body: Option<serde_json::Value>) -> Result<serde_json::Value, reqwest::Error> {
    let client = reqwest::Client::new();
    let mut req = client
        .request(body.as_ref().map(|_| reqwest::Method::POST).unwrap_or(reqwest::Method::GET), format!("http://localhost:3000{path}"))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json");
    if let Some(b) = body {
        req = req.json(&b);
    }
    req.send().await?.error_for_status()?.json().await
}
```

---

## Rate Limiting

The API enforces token-bucket rate limits to protect backend resources.

| Scope | Limit | Header / Key |
|-------|-------|--------------|
| Per IP address | 100 requests / minute | Source IP |
| Per API key | 1,000 requests / minute | `x-api-key` header |

If the limit is exceeded, the API returns:

```json
{ "error": "Rate limit exceeded" }
```

with HTTP status `429 Too Many Requests`.

### Handling Rate Limits

- **Prefer API keys** for production traffic to benefit from the higher limit.
- **Implement client-side backoff** when receiving `429` (see [Retry Logic](#retry-logic)).
- **Cache read-heavy responses** such as `GET /ip/{ip_id}` to reduce request volume.

---

## Error Handling

All errors return a consistent JSON structure:

```json
{ "error": "human-readable message" }
```

### HTTP Status Codes

| Code | Meaning | Example |
|------|---------|---------|
| `400` | Bad Request | Zero hash, duplicate hash, invalid state transition |
| `401` | Unauthorized | Missing or invalid JWT token |
| `404` | Not Found | IP record or swap not found |
| `429` | Too Many Requests | Rate limit exceeded |
| `500` | Internal Server Error | Token creation failure or unhandled server error |

### Client Parsing Patterns

**TypeScript**
```typescript
try {
  const data = await apiRequest("/ip/1", token);
} catch (err: any) {
  if (err.status === 429) {
    const retryAfter = err.headers.get("Retry-After") || "60";
    await sleep(parseInt(retryAfter) * 1000);
  } else {
    const body = await err.json();
    console.error("API error:", body.error);
  }
}
```

**Python**
```python
try:
    data = api_request("/ip/1", token)
except requests.HTTPError as e:
    if e.response.status_code == 429:
        retry_after = int(e.response.headers.get("Retry-After", 60))
        time.sleep(retry_after)
    else:
        print("API error:", e.response.json()["error"])
```

**Rust**
```rust
match api_request("/ip/1", token, None).await {
    Ok(data) => println!("{}", data),
    Err(e) if e.status() == Some(reqwest::StatusCode::TOO_MANY_REQUESTS) => {
        // backoff and retry
    }
    Err(e) => eprintln!("API error: {}", e),
}
```

---

## Retry Logic

Transient failures (network blips, `429`, `500`, `502`, `503`, `504`) should be retried with an exponential backoff strategy.

### Recommended Retry Policy

- **Max retries:** 5
- **Base delay:** 1 second
- **Backoff:** exponential (`delay = base * 2^attempt`)
- **Jitter:** add randomness (`delay * (0.5 + random())`) to avoid thundering herd
- **Retry on:** `429`, `500`, `502`, `503`, `504`, and network timeouts
- **Do NOT retry on:** `400`, `401`, `404` (these indicate client-side or auth issues)

### TypeScript Retry Example

```typescript
async function fetchWithRetry(
  input: RequestInfo,
  init?: RequestInit,
  maxRetries = 5
): Promise<Response> {
  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      const res = await fetch(input, init);
      if (res.ok) return res;
      if (res.status === 429 || res.status >= 500) {
        const delay = Math.min(1000 * 2 ** attempt + Math.random() * 1000, 30000);
        await new Promise((r) => setTimeout(r, delay));
        continue;
      }
      throw new Error(`HTTP ${res.status}`);
    } catch (err) {
      if (attempt === maxRetries) throw err;
      await new Promise((r) => setTimeout(r, 1000 * 2 ** attempt));
    }
  }
  throw new Error("Max retries exceeded");
}
```

### Python Retry Example

```python
import random
import time

def fetch_with_retry(method, url, headers=None, json=None, max_retries=5):
    for attempt in range(max_retries + 1):
        try:
            resp = requests.request(method, url, headers=headers, json=json, timeout=10)
            if resp.status_code in (429, 500, 502, 503, 504):
                delay = min(2 ** attempt + random.random(), 30)
                time.sleep(delay)
                continue
            resp.raise_for_status()
            return resp
        except requests.RequestException as e:
            if attempt == max_retries:
                raise
            time.sleep(2 ** attempt)
```

### Rust Retry Example

```rust
use std::time::Duration;
use tokio::time::sleep;
use rand::Rng;

async fn request_with_retry(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    token: &str,
    body: Option<serde_json::Value>,
    max_retries: u32,
) -> Result<reqwest::Response, reqwest::Error> {
    for attempt in 0..=max_retries {
        let mut req = client
            .request(method.clone(), url)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(CONTENT_TYPE, "application/json");
        if let Some(ref b) = body {
            req = req.json(b);
        }
        match req.send().await {
            Ok(resp) if resp.status().is_success() => return Ok(resp),
            Ok(resp) if matches!(resp.status().as_u16(), 429 | 500..=504) => {
                let delay = Duration::from_millis(1000 * 2u64.pow(attempt) + rand::thread_rng().gen_range(0..1000));
                sleep(delay).await;
            }
            Ok(resp) => return Ok(resp),
            Err(e) if attempt == max_retries => return Err(e),
            Err(_) => sleep(Duration::from_secs(2u64.pow(attempt))).await,
        }
    }
    unreachable!()
}
```

---

## Common Use Cases

### 1. Commit IP

Register a new intellectual property commitment on-chain.

**Endpoint:** `POST /ip/commit`

**Request Body:**
```json
{
  "owner": "GABC...",
  "commitment_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
}
```

**TypeScript**
```typescript
async function commitIp(token: string, owner: string, hash: string): Promise<number> {
  const res = await fetchWithRetry("http://localhost:3000/ip/commit", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({ owner, commitment_hash: hash }),
  });
  return (await res.json()) as number; // ip_id
}
```

**Python**
```python
def commit_ip(token: str, owner: str, commitment_hash: str) -> int:
    resp = fetch_with_retry(
        "POST",
        "http://localhost:3000/ip/commit",
        headers={"Authorization": f"Bearer {token}"},
        json={"owner": owner, "commitment_hash": commitment_hash},
    )
    return resp.json()
```

**Rust**
```rust
async fn commit_ip(client: &reqwest::Client, token: &str, owner: &str, hash: &str) -> Result<u64, reqwest::Error> {
    let res = request_with_retry(
        client,
        reqwest::Method::POST,
        "http://localhost:3000/ip/commit",
        token,
        Some(serde_json::json!({"owner": owner, "commitment_hash": hash})),
        5,
    ).await?;
    res.json().await
}
```

---

### 2. Initiate Swap

A seller initiates a patent sale by specifying the IP, buyer, price, and payment token.

**Endpoint:** `POST /swap/initiate`

**Request Body:**
```json
{
  "ip_registry_id": "CDEF...",
  "ip_id": 1,
  "seller": "GABC...",
  "price": 100000000,
  "buyer": "GXYZ...",
  "token": "CASD..."
}
```

**TypeScript**
```typescript
async function initiateSwap(
  token: string,
  ipRegistryId: string,
  ipId: number,
  seller: string,
  price: number,
  buyer: string,
  tokenAddress: string
): Promise<number> {
  const res = await fetchWithRetry("http://localhost:3000/swap/initiate", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({ ip_registry_id: ipRegistryId, ip_id: ipId, seller, price, buyer, token: tokenAddress }),
  });
  return (await res.json()) as number; // swap_id
}
```

**Python**
```python
def initiate_swap(token: str, ip_registry_id: str, ip_id: int, seller: str, price: int, buyer: str, token_address: str) -> int:
    resp = fetch_with_retry(
        "POST",
        "http://localhost:3000/swap/initiate",
        headers={"Authorization": f"Bearer {token}"},
        json={
            "ip_registry_id": ip_registry_id,
            "ip_id": ip_id,
            "seller": seller,
            "price": price,
            "buyer": buyer,
            "token": token_address,
        },
    )
    return resp.json()
```

**Rust**
```rust
async fn initiate_swap(
    client: &reqwest::Client,
    token: &str,
    ip_registry_id: &str,
    ip_id: u64,
    seller: &str,
    price: i128,
    buyer: &str,
    token_address: &str,
) -> Result<u64, reqwest::Error> {
    let res = request_with_retry(
        client,
        reqwest::Method::POST,
        "http://localhost:3000/swap/initiate",
        token,
        Some(serde_json::json!({
            "ip_registry_id": ip_registry_id,
            "ip_id": ip_id,
            "seller": seller,
            "price": price,
            "buyer": buyer,
            "token": token_address,
        })),
        5,
    ).await?;
    res.json().await
}
```

---

### 3. Accept Swap

The buyer accepts a pending swap, moving it to the `Accepted` state and locking the payment in escrow.

**Endpoint:** `POST /swap/{swap_id}/accept`

**Request Body:**
```json
{ "buyer": "GXYZ..." }
```

**TypeScript**
```typescript
async function acceptSwap(token: string, swapId: number, buyer: string): Promise<void> {
  const res = await fetchWithRetry(`http://localhost:3000/swap/${swapId}/accept`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({ buyer }),
  });
  if (!res.ok) throw new Error(`Accept failed: ${res.status}`);
}
```

**Python**
```python
def accept_swap(token: str, swap_id: int, buyer: str) -> None:
    resp = fetch_with_retry(
        "POST",
        f"http://localhost:3000/swap/{swap_id}/accept",
        headers={"Authorization": f"Bearer {token}"},
        json={"buyer": buyer},
    )
    resp.raise_for_status()
```

**Rust**
```rust
async fn accept_swap(client: &reqwest::Client, token: &str, swap_id: u64, buyer: &str) -> Result<(), reqwest::Error> {
    request_with_retry(
        client,
        reqwest::Method::POST,
        &format!("http://localhost:3000/swap/{swap_id}/accept"),
        token,
        Some(serde_json::json!({"buyer": buyer})),
        5,
    ).await?;
    Ok(())
}
```

---

### 4. Reveal Key

The seller reveals the decryption key (secret + blinding factor) to complete the swap. Payment is released from escrow and the swap status becomes `Completed`.

**Endpoint:** `POST /swap/{swap_id}/reveal`

**Request Body:**
```json
{
  "caller": "GABC...",
  "secret": "aabbccdd...",
  "blinding_factor": "11223344..."
}
```

**TypeScript**
```typescript
async function revealKey(
  token: string,
  swapId: number,
  caller: string,
  secret: string,
  blindingFactor: string
): Promise<void> {
  const res = await fetchWithRetry(`http://localhost:3000/swap/${swapId}/reveal`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({ caller, secret, blinding_factor: blindingFactor }),
  });
  if (!res.ok) throw new Error(`Reveal failed: ${res.status}`);
}
```

**Python**
```python
def reveal_key(token: str, swap_id: int, caller: str, secret: str, blinding_factor: str) -> None:
    resp = fetch_with_retry(
        "POST",
        f"http://localhost:3000/swap/{swap_id}/reveal",
        headers={"Authorization": f"Bearer {token}"},
        json={"caller": caller, "secret": secret, "blinding_factor": blinding_factor},
    )
    resp.raise_for_status()
```

**Rust**
```rust
async fn reveal_key(
    client: &reqwest::Client,
    token: &str,
    swap_id: u64,
    caller: &str,
    secret: &str,
    blinding_factor: &str,
) -> Result<(), reqwest::Error> {
    request_with_retry(
        client,
        reqwest::Method::POST,
        &format!("http://localhost:3000/swap/{swap_id}/reveal"),
        token,
        Some(serde_json::json!({
            "caller": caller,
            "secret": secret,
            "blinding_factor": blinding_factor,
        })),
        5,
    ).await?;
    Ok(())
}
```

---

## Best Practices

1. **Store tokens securely.** Keep the refresh token in a secure vault (e.g., OS keychain, encrypted storage). Never commit tokens to source control.
2. **Handle 401 gracefully.** If the access token expires, refresh it automatically using the stored refresh token.
3. **Monitor rate limits.** Track your request rate and proactively slow down if you approach the 100 or 1,000 req/min thresholds.
4. **Validate inputs client-side.** Ensure `commitment_hash`, `secret`, and `blinding_factor` are valid 64-character hex strings before sending to reduce `400` errors.
5. **Idempotency.** Most write endpoints are backed by deterministic on-chain transactions; re-submitting with the same parameters may fail with `400` (e.g., duplicate hash). Design your client to treat `400` as final and not retry blindly.
6. **Webhook verification.** If you consume webhooks, verify the `X-Webhook-Signature` header using your registered secret to ensure authenticity.
7. **Logging.** Log request IDs, response times, and error bodies to aid debugging. Redact sensitive fields (`secret`, `blinding_factor`, tokens) from logs.

