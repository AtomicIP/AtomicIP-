# API Migration Guide

> Closes #1060. How the Atomic Patent API is versioned, how deprecations are
> announced, what changed between versions, and how to migrate client code safely.

Related: [SDK Versioning](sdk-versioning.md) · [API Reference](api-reference.md) ·
[Error Response Schema](error-response-schema.md) · source of truth:
`api-server/src/versioning.rs`

---

## 1. Versioning Strategy

The API follows **Semantic Versioning** (`MAJOR.MINOR.PATCH`):

| Change type | Version bump | URL prefix | Client action |
|-------------|-------------|------------|---------------|
| Bug fix, no contract change | PATCH (`1.0.0 → 1.0.1`) | unchanged | None |
| New optional field / new endpoint | MINOR (`1.0.0 → 1.1.0`) | unchanged (`/v1/`) | Optional adoption |
| Removed/renamed field, changed type or semantics | MAJOR (`1.x → 2.0.0`) | new prefix (`/v2/`) | Code changes required |

Two mechanisms select a version:

1. **URL prefix** — `/v1/...`. Breaking changes only ever ship under a new prefix, so
   `/v1/` clients are never silently broken.
2. **`Accept-Version` header** — selects a minor version inside a prefix
   (e.g. `Accept-Version: 1.1.0`). Omitted ⇒ `CURRENT_VERSION`.

Every response carries `API-Version: <current>`. Unsupported versions return
`406 Not Acceptable`:

```json
{
  "error": "API version '2.0.0' is not supported. Supported versions: 1.0.0, 1.1.0",
  "requested_version": "2.0.0",
  "supported_versions": ["1.0.0", "1.1.0"],
  "current_version": "1.0.0"
}
```

`GET /version` returns the current version, supported versions, features and the
deprecation policy — use it for runtime capability detection.

## 2. Deprecation Timeline

| Phase | Timing | What happens |
|-------|--------|--------------|
| **Announce** | T | Changelog entry, this guide updated, `Deprecation: true` header on the old version |
| **Notice period** | T → T + 6 months (min. 180 days) | Both versions served; `Sunset: <RFC 2822 date>` header on the old version |
| **Brownouts** (recommended) | Last 30 days | Short scheduled windows where the old version returns `410 Gone`, to surface unmigrated clients |
| **Sunset** | T + ≥ 6 months | Old version removed from `SUPPORTED_VERSIONS`; requests return `406` |

### Current status

| Version | Status | Deprecation | Sunset |
|---------|--------|-------------|--------|
| `1.0.0` | **Current / stable** | — | — |
| `1.1.0` | Supported | `Deprecation: true` header (non-current) | `Sun, 31 Dec 2027 23:59:59 GMT` |
| `2.0.0` | Planned | — | — |

### Detecting deprecation in clients

```ts
const res = await fetch(`${BASE}/v1/swap/${id}`, { headers: { 'Accept-Version': '1.1.0' } });
if (res.headers.get('Deprecation') === 'true') {
  logger.warn('API version deprecated', {
    version: res.headers.get('API-Version'),
    sunset: res.headers.get('Sunset'),
  });
}
```

Alert on this in CI/staging so deprecations are noticed well before sunset.

## 3. Breaking Changes per Version

### Unversioned → `v1` (1.0.0)

The original routes (`/ip/commit`, `/swap/initiate`, …) are still mounted for
backward compatibility, but all new integrations should use `/v1/`:

| Legacy route | v1 route |
|--------------|----------|
| `POST /ip/commit` | `POST /v1/ip/commit` |
| `GET /ip/{ip_id}` | `GET /v1/ip/{ip_id}` |
| `POST /ip/transfer` | `POST /v1/ip/transfer` |
| `POST /ip/verify` | `POST /v1/ip/verify` |
| `GET /ip/owner/{owner}` | `GET /v1/ip/owner/{owner}` |
| `GET /ip/owner/{owner}/cursor` | `GET /v1/ip/owner/{owner}/cursor` |
| `POST /swap/initiate` | `POST /v1/swap/initiate` |
| `POST /swap/batch-initiate` | `POST /v1/swap/batch-initiate` |
| `POST /swap/{id}/accept` | `POST /v1/swap/{id}/accept` |
| `POST /swap/{id}/reveal` | `POST /v1/swap/{id}/reveal` |
| `POST /swap/{id}/cancel` | `POST /v1/swap/{id}/cancel` |
| `POST /swap/{id}/cancel-expired` | `POST /v1/swap/{id}/cancel-expired` |
| `GET /swap/{id}` | `GET /v1/swap/{id}` |

Behavioural changes introduced alongside v1:

- **Request signing required** on write endpoints (commit, transfer, swap
  initiate/accept/reveal/cancel). Unsigned requests are rejected.
- **Structured error body** — errors follow [error-response-schema.md](error-response-schema.md)
  instead of plain strings.
- **Cursor pagination** — `/ip/owner/{owner}/cursor` for large owners; offset listing
  is retained but not recommended for large result sets.

### 1.0.0 → 1.1.0 (minor, non-breaking)

Additive only. Clients on 1.0.0 need no changes. Opt in with `Accept-Version: 1.1.0`.

### 1.x → 2.0.0 (planned)

Breaking changes will be listed here before release, each with:
the old shape, the new shape, the reason, and a code example. Anticipated areas:

- Removal of the unversioned legacy routes.
- Consolidating dispute/arbitration endpoints under `/v2/swap/{id}/dispute/*`.

## 4. Migration Code Examples

### 4.1 Legacy routes → `/v1/`

```diff
- const res = await fetch(`${BASE}/swap/initiate`, { method: 'POST', body });
+ const res = await fetch(`${BASE}/v1/swap/initiate`, {
+   method: 'POST',
+   body,
+   headers: {
+     'Content-Type': 'application/json',
+     'Accept-Version': '1.0.0',
+     ...signRequest(body),          // request signing, see integration-guide.md
+   },
+ });
```

### 4.2 Centralise the base path and version

```ts
// apiClient.ts
export const API_VERSION = '1.0.0';
export const API_PREFIX = '/v1';

export async function api(path: string, init: RequestInit = {}) {
  return fetch(`${BASE}${API_PREFIX}${path}`, {
    ...init,
    headers: { 'Accept-Version': API_VERSION, ...init.headers },
  });
}
```

A major upgrade then becomes a two-constant change plus fixing any changed payloads.

### 4.3 Handling structured errors

```diff
- if (!res.ok) throw new Error(await res.text());
+ if (!res.ok) {
+   const err = await res.json();          // { error, code, ... }
+   throw new ApiError(res.status, err);
+ }
```

### 4.4 Offset → cursor pagination

```ts
let cursor: string | undefined;
do {
  const q = cursor ? `?cursor=${encodeURIComponent(cursor)}` : '';
  const page = await (await api(`/ip/owner/${owner}/cursor${q}`)).json();
  handle(page.items);
  cursor = page.next_cursor;
} while (cursor);
```

### 4.5 Rust client

```rust
let resp = client
    .post(format!("{base}/v1/ip/commit"))
    .header("Accept-Version", "1.0.0")
    .json(&body)
    .send()
    .await?;
if resp.headers().get("Deprecation").map(|v| v == "true").unwrap_or(false) {
    tracing::warn!(sunset = ?resp.headers().get("Sunset"), "API version deprecated");
}
```

## 5. Fallback Strategies

1. **Version negotiation with fallback** — try the newest version, fall back on `406`:

   ```ts
   async function negotiated(path: string, init: RequestInit = {}) {
     for (const v of ['1.1.0', '1.0.0']) {
       const res = await fetch(`${BASE}/v1${path}`, {
         ...init, headers: { ...init.headers, 'Accept-Version': v },
       });
       if (res.status !== 406) return res;
     }
     throw new Error('No mutually supported API version');
   }
   ```

2. **Capability detection** — read `GET /version` at start-up and enable features
   based on `features[]` rather than hard-coding version numbers.
3. **Tolerant readers** — ignore unknown JSON fields so minor versions never break you.
4. **Feature flags** — gate new-version code paths behind a flag so you can roll back
   without redeploying.
5. **Staged rollout** — migrate a canary percentage of traffic first, compare error
   rates via `/metrics` (`http_errors_total`), then ramp up.
6. **Direct contract fallback** — the API is a gateway; if it is unavailable, clients
   can invoke the Soroban contracts directly through the Stellar RPC
   ([integration-guide.md](integration-guide.md)).
7. **Pin before sunset** — never leave `Accept-Version` unset in production; an
   unpinned client silently tracks `CURRENT_VERSION`.

## 6. Migration Checklist

- [ ] All calls use the `/v1/` prefix
- [ ] `Accept-Version` pinned in a single constant
- [ ] Write requests signed
- [ ] Errors parsed as structured JSON
- [ ] `Deprecation` / `Sunset` headers logged and alerted on
- [ ] Large listings use cursor pagination
- [ ] Fallback path tested in staging
- [ ] SDK upgraded to a version compatible with the target API ([sdk-versioning.md](sdk-versioning.md))
