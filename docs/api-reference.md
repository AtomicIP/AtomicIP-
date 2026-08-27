# IP Registry API Reference

Complete API documentation for the IP Registry smart contract.

---

## `commit_ip`

Timestamp a new IP commitment on-chain.

### Signature

```rust
pub fn commit_ip(env: Env, owner: Address, commitment_hash: BytesN<32>) -> u64
```

### Parameters

| Parameter | Type | Description |
|---|---|---|
| `env` | `Env` | Soroban environment (injected automatically) |
| `owner` | `Address` | The address that owns the IP. Must authorize the transaction. |
| `commitment_hash` | `BytesN<32>` | 32-byte cryptographic hash: `sha256(secret \|\| blinding_factor)` |

### Returns

`u64` — The unique IP ID assigned to this commitment. IDs start at 1 and increment sequentially.

### Panics

| Error | Code | Condition |
|---|---|---|
| `ZeroCommitmentHash` | 2 | `commitment_hash` is all zeros |
| `CommitmentAlreadyRegistered` | 3 | `commitment_hash` already exists on-chain |
| Auth error | — | `owner` does not authorize the transaction |

### Authorization

Requires `owner.require_auth()` — the transaction must be signed by the owner's private key.

### Example (Rust SDK)

```rust
let owner = Address::from_string("GABC...");
let secret = BytesN::from_array(&env, &[/* 32 bytes */]);
let blinding_factor = BytesN::from_array(&env, &[/* 32 random bytes */]);

let mut preimage = Bytes::new(&env);
preimage.append(&secret.into());
preimage.append(&blinding_factor.into());
let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

let ip_id = registry.commit_ip(&owner, &commitment_hash);
```

### Example (REST API)

**POST** `/ip/commit`

**Request Body:**
```json
{
  "owner": "GABC...",
  "commitment_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
}
```

**Response (200 OK):**
```json
1
```

---

## `batch_commit_ip`

Commit multiple IP hashes from the same owner in a single transaction. Reduces gas fees.

### Signature

```rust
pub fn batch_commit_ip(env: Env, owner: Address, hashes: Vec<BytesN<32>>) -> Vec<u64>
```

### Parameters

| Parameter | Type | Description |
|---|---|---|
| `env` | `Env` | Soroban environment |
| `owner` | `Address` | Owner address (requires auth) |
| `hashes` | `Vec<BytesN<32>>` | Vector of commitment hashes to register |

### Returns

`Vec<u64>` — Vector of assigned sequential IP IDs.

### Panics

Same as `commit_ip` — panics if any hash is zero or already registered.

### Example (Rust SDK)

```rust
let hashes = Vec::from_array(&env, [hash1, hash2, hash3]);
let ip_ids = registry.batch_commit_ip(&owner, &hashes);
// ip_ids = [1, 2, 3]
```

### Example (REST API)

**POST** `/ip/batch`

**Request Body:**
```json
{
  "owner": "GABC...",
  "hashes": [
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "5feceb66ffc86f38d952786c6d696c79c2dbc239dd4e91b46729d73a27fb57e9"
  ]
}
```

**Response (200 OK):**
```json
[1, 2]
```

---

## `batch_commit_ip_anonymous`

Commit multiple IP hashes anonymously in a single transaction. The contract stores a blinded owner identifier alongside each commitment; the on-chain `owner` field is set to the contract address to avoid exposing the submitter.

### Signature

```rust
pub fn batch_commit_ip_anonymous(
    env: Env,
    blinded_owner: BytesN<32>,
    commitment_hashes: Vec<BytesN<32>>,
) -> Vec<u64>
```

### Parameters

| Parameter | Type | Description |
|---|---|---|
| `env` | `Env` | Soroban environment |
| `blinded_owner` | `BytesN<32>` | Off-chain blinded owner identifier (e.g. `sha256(owner \|\| nonce)`). Stored per commitment for later ownership proof. |
| `commitment_hashes` | `Vec<BytesN<32>>` | Non-empty vector of commitment hashes to register anonymously. |

### Returns

`Vec<u64>` — Assigned sequential IP IDs in the same order as the input hashes.

### Panics

| Error | Code | Condition |
|---|---|---|
| `ZeroCommitmentHash` | 2 | `commitment_hashes` is empty, or any hash is all zeros |
| `CommitmentAlreadyRegistered` | 3 | Any hash is already registered (including duplicates within the same batch) |

### Auth Model

No caller authorization is required. The submitter's identity is intentionally not recorded on-chain.

### Events

One event is emitted per commitment hash:

- **Topics:** `(symbol_short!("ip_commit_anon"), contract_address)`
- **Data:** `(ip_id: u64, timestamp: u64, blinded_owner: BytesN<32>)`

### Storage

Per commitment hash, two persistent storage keys are written:

| Key | Value | Purpose |
|---|---|---|
| `CommitmentOwner(hash)` | contract address | Global duplicate guard |
| `AnonymousOwner(hash)` | `blinded_owner` | Ownership proof pointer |

Anonymous commits do **not** populate `OwnerIps` — they will not appear in `list_ip_by_owner` for any address.

### Example (Rust SDK)

```rust
// Construct blinded owner: sha256(real_owner_bytes || random_nonce)
let mut preimage = Bytes::new(&env);
preimage.append(&owner_bytes);
preimage.append(&nonce_bytes);
let blinded_owner: BytesN<32> = env.crypto().sha256(&preimage).into();

let hashes = Vec::from_array(&env, [hash1, hash2]);
let ip_ids = registry.batch_commit_ip_anonymous(&blinded_owner, &hashes);
// ip_ids = [1, 2]
```

### Example (REST API)

**POST** `/ip/batch/anonymous`

**Request Body:**
```json
{
  "blinded_owner": "a1b2c3d4...",
  "commitment_hashes": [
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "5feceb66ffc86f38d952786c6d696c79c2dbc239dd4e91b46729d73a27fb57e9"
  ]
}
```

**Response (200 OK):**
```json
[1, 2]
```

---

## `get_anonymous_owner`

Retrieve the blinded owner identifier stored for an anonymous commitment.

### Signature

```rust
pub fn get_anonymous_owner(env: Env, commitment_hash: BytesN<32>) -> Option<BytesN<32>>
```

### Parameters

| Parameter | Type | Description |
|---|---|---|
| `env` | `Env` | Soroban environment |
| `commitment_hash` | `BytesN<32>` | The commitment hash to look up |

### Returns

`Option<BytesN<32>>` — The blinded owner identifier if the hash was registered via `batch_commit_ip_anonymous`, or `None` if no anonymous owner record exists (e.g. the hash was committed via `commit_ip`).

### Panics

This function does not panic.

### Example (Rust SDK)

```rust
let blinded = registry.get_anonymous_owner(&commitment_hash);
match blinded {
    Some(b) => println!("Blinded owner: {:?}", b),
    None => println!("Not an anonymous commitment"),
}
```

---


## `get_ip`

Retrieve an IP record by ID.

### Signature

```rust
pub fn get_ip(env: Env, ip_id: u64) -> IpRecord
```

### Parameters

| Parameter | Type | Description |
|---|---|---|
| `env` | `Env` | Soroban environment |
| `ip_id` | `u64` | The unique identifier of the IP to retrieve |

### Returns

`IpRecord` struct:

```rust
pub struct IpRecord {
    pub ip_id: u64,
    pub owner: Address,
    pub commitment_hash: BytesN<32>,
    pub timestamp: u64,
    pub revoked: bool,
}
```

| Field | Type | Description |
|---|---|---|
| `ip_id` | `u64` | Unique identifier |
| `owner` | `Address` | Current owner's address |
| `commitment_hash` | `BytesN<32>` | The cryptographic commitment |
| `timestamp` | `u64` | Ledger timestamp when IP was committed |
| `revoked` | `bool` | Whether the IP has been revoked |

### Panics

| Error | Code | Condition |
|---|---|---|
| `IpNotFound` | 1 | IP record does not exist |

### Example (Rust SDK)

```rust
let record = registry.get_ip(&ip_id);
println!("Owner: {}", record.owner);
println!("Timestamp: {}", record.timestamp);
```

### Example (REST API)

**GET** `/ip/1`

**Response (200 OK):**
```json
{
  "ip_id": 1,
  "owner": "GABC...",
  "commitment_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  "timestamp": 1713994200,
  "revoked": false
}
```

---

## `verify_commitment`

Verify that a secret and blinding factor match a stored commitment hash.

### Signature

```rust
pub fn verify_commitment(
    env: Env,
    ip_id: u64,
    secret: BytesN<32>,
    blinding_factor: BytesN<32>,
) -> bool
```

### Parameters

| Parameter | Type | Description |
|---|---|---|
| `env` | `Env` | Soroban environment |
| `ip_id` | `u64` | The IP to verify |
| `secret` | `BytesN<32>` | The 32-byte secret used to create the commitment |
| `blinding_factor` | `BytesN<32>` | The 32-byte blinding factor used to create the commitment |

### Returns

`bool` — `true` if `sha256(secret || blinding_factor)` matches the stored commitment hash, `false` otherwise.

### Panics

| Error | Code | Condition |
|---|---|---|
| `IpNotFound` | 1 | IP record does not exist |

### Example (Rust SDK)

```rust
let is_valid = registry.verify_commitment(&ip_id, &secret, &blinding_factor);
if is_valid {
    println!("Commitment verified!");
}
```

### Example (REST API)

**POST** `/ip/verify`

**Request Body:**
```json
{
  "ip_id": 1,
  "secret": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  "blinding_factor": "5feceb66ffc86f38d952786c6d696c79c2dbc239dd4e91b46729d73a27fb57e9"
}
```

**Response (200 OK):**
```json
{
  "valid": true
}
```

---

## `list_ip_by_owner`

List all IP IDs owned by an address.

### Signature

```rust
pub fn list_ip_by_owner(env: Env, owner: Address) -> Vec<u64>
```

### Parameters

| Parameter | Type | Description |
|---|---|---|
| `env` | `Env` | Soroban environment |
| `owner` | `Address` | The address to list IPs for |

### Returns

`Vec<u64>` — Vector of all IP IDs owned by the address. Returns an empty vector if the address has no IPs.

### Panics

This function does not panic.

### Example (Rust SDK)

```rust
let ip_ids = registry.list_ip_by_owner(&owner);
for ip_id in ip_ids.iter() {
    let record = registry.get_ip(&ip_id);
    println!("IP {}: {}", ip_id, record.commitment_hash);
}
```

### Example (REST API)

**GET** `/ip/owner/GABC...`

**Response (200 OK):**
```json
{
  "ip_ids": [1, 2, 5]
}
```

---

## `transfer_ip`

Transfer IP ownership to a new address.

### Signature

```rust
pub fn transfer_ip(env: Env, ip_id: u64, new_owner: Address)
```

### Parameters

| Parameter | Type | Description |
|---|---|---|
| `env` | `Env` | Soroban environment |
| `ip_id` | `u64` | The IP to transfer |
| `new_owner` | `Address` | The address that will become the new owner |

### Returns

This function does not return a value.

### Panics

| Error | Code | Condition |
|---|---|---|
| `IpNotFound` | 1 | IP record does not exist |
| Auth error | — | Current owner does not authorize the transaction |

### Authorization

Requires `record.owner.require_auth()` — the current owner must sign the transaction.

### Example (Rust SDK)

```rust
registry.transfer_ip(&ip_id, &new_owner);
```

### Example (REST API)

**POST** `/ip/transfer`

**Request Body:**
```json
{
  "ip_id": 1,
  "new_owner": "GDEF..."
}
```

**Response (200 OK):**
```json
{}
```

---

## `revoke_ip`

Revoke an IP record, marking it as invalid. Revoked IPs cannot be swapped.

### Signature

```rust
pub fn revoke_ip(env: Env, ip_id: u64)
```

### Parameters

| Parameter | Type | Description |
|---|---|---|
| `env` | `Env` | Soroban environment |
| `ip_id` | `u64` | The IP to revoke |

### Returns

This function does not return a value.

### Panics

| Error | Code | Condition |
|---|---|---|
| `IpNotFound` | 1 | IP record does not exist |
| `IpAlreadyRevoked` | 4 | IP is already revoked |
| Auth error | — | Owner does not authorize the transaction |

### Authorization

Requires `record.owner.require_auth()` — only the current owner can revoke.

### Example (Rust SDK)

```rust
registry.revoke_ip(&ip_id);
```

### Example (REST API)

**POST** `/ip/revoke` (Note: Custom endpoint for revocation)

**Request Body:**
```json
{
  "ip_id": 1
}
```

**Response (200 OK):**
```json
{}
```

---

## `is_ip_owner`

Check if an address owns a specific IP.

### Signature

```rust
pub fn is_ip_owner(env: Env, ip_id: u64, address: Address) -> bool
```

### Parameters

| Parameter | Type | Description |
|---|---|---|
| `env` | `Env` | Soroban environment |
| `ip_id` | `u64` | The IP to check |
| `address` | `Address` | The address to check for ownership |

### Returns

`bool` — `true` if the address owns the IP, `false` otherwise. Returns `false` if the IP does not exist.

### Panics

This function does not panic.

### Example

```rust
if registry.is_ip_owner(&ip_id, &address) {
    println!("Address owns this IP");
}
```

---

## Error Codes

| Error | Code | Description |
|---|---|---|
| `IpNotFound` | 1 | IP record does not exist |
| `ZeroCommitmentHash` | 2 | Commitment hash is all zeros |
| `CommitmentAlreadyRegistered` | 3 | Commitment hash already registered |
| `IpAlreadyRevoked` | 4 | IP is already revoked |
| `UnauthorizedUpgrade` | 5 | Caller is not admin (upgrade only) |

---

## Events

### `ip_commit`

Emitted when a new IP is committed.

**Topics:** `(symbol_short!("ip_commit"), owner: Address)`  
**Data:** `(ip_id: u64, timestamp: u64)`

---

## Storage Keys

| Key | Type | Description |
|---|---|---|
| `IpRecord(u64)` | Persistent | Stores IP record by ID |
| `OwnerIps(Address)` | Persistent | Maps owner → Vec of IP IDs |
| `NextId` | Persistent | Next available IP ID (monotonic counter) |
| `CommitmentOwner(BytesN<32>)` | Persistent | Maps commitment hash → owner (duplicate detection) |
| `Admin` | Persistent | Admin address for upgrades |

---

## TTL Management

All persistent storage entries are extended with a TTL of **~1 year** (6,307,200 ledgers at 5s/ledger).

See [TTL_MANAGEMENT.md](../TTL_MANAGEMENT.md) for details.

---

## Related Documentation

- [Commitment Scheme](commitment-scheme.md) — How to construct valid commitment hashes
- [Atomic Swap Flow](atomic-swap.md) — How to sell IP using atomic swaps
- [Security Considerations](security.md) — Best practices for secret management

---

## Tiered Access Control

IP owners can grant other addresses tiered read/verify/transfer access without transferring ownership.

### Access Tiers

| Level | Name | Permissions |
|---|---|---|
| `1` | **view** | Read IP metadata |
| `2` | **verify** | View + verify the commitment |
| `3` | **transfer** | View + verify + initiate transfer |

Tiers are hierarchical: a grantee with level 3 satisfies checks for levels 1 and 2. The owner always has full access (level 3) regardless of grants.

---

### `grant_ip_access`

Grant tiered access to an IP for a third party. Owner-only. Granting to an address that already has a grant updates the level.

```rust
pub fn grant_ip_access(env: Env, ip_id: u64, grantee: Address, access_level: u32)
```

| Parameter | Type | Description |
|---|---|---|
| `ip_id` | `u64` | The IP to grant access to |
| `grantee` | `Address` | The address receiving access |
| `access_level` | `u32` | `1` = view, `2` = verify, `3` = transfer |

**Panics:** `Unauthorized` (6) if `access_level` is 0 or > 3, or caller is not the owner.

**Event:** `(symbol_short!("ac_grant"), ip_id)` → `(grantee, access_level)`

```rust
// Grant verify access to a partner
registry.grant_ip_access(&ip_id, &partner, &2u32);
```

---

### `revoke_ip_access`

Revoke access from a grantee. Owner-only. No-op if the grantee has no grant.

```rust
pub fn revoke_ip_access(env: Env, ip_id: u64, grantee: Address)
```

**Event:** `(symbol_short!("ac_revoke"), ip_id)` → `grantee`

```rust
registry.revoke_ip_access(&ip_id, &partner);
```

---

### `check_ip_access`

Check whether an address has at least the required access level for an IP.

```rust
pub fn check_ip_access(env: Env, ip_id: u64, grantee: Address, required_level: u32) -> bool
```

Returns `true` if the grantee's level ≥ `required_level`, or if `grantee` is the owner.

```rust
if registry.check_ip_access(&ip_id, &caller, &2u32) {
    // caller can verify the commitment
}
```

---

### `get_ip_access_grants`

Return all active access grants for an IP.

```rust
pub fn get_ip_access_grants(env: Env, ip_id: u64) -> Vec<IpAccessGrant>
```

Returns a `Vec<IpAccessGrant>` where each entry has `grantee: Address` and `access_level: u32`.

---

## Issue #458 / #780 — Batch Verification

Two batch entry points, named for exactly what they disclose. See
`docs/commitment-scheme.md` for the full write-up of both, including the
Fiat-Shamir Schnorr construction behind the hiding path.

### `reveal_and_verify_commitments`

Verify multiple IP commitments in a single call by having the caller reveal the `secret` and `blinding_factor` used to create each one. Each request recomputes `sha256(secret || blinding_factor)` and checks it against the stored commitment hash using constant-time comparison. **This discloses both values on-chain — it is not a zero-knowledge proof.** Use it only when the caller intends to disclose; for a proof that discloses nothing, see `batch_verify_commitments` below.

```rust
pub fn reveal_and_verify_commitments(env: Env, requests: Vec<VerifyRequest>) -> Vec<VerifyResult>
```

#### `VerifyRequest`

| Field | Type | Description |
|---|---|---|
| `ip_id` | `u64` | The IP ID to verify |
| `secret` | `BytesN<32>` | The secret used when committing — disclosed on-chain |
| `blinding_factor` | `BytesN<32>` | The blinding factor used when committing — disclosed on-chain |

#### `VerifyResult`

| Field | Type | Description |
|---|---|---|
| `ip_id` | `u64` | The IP ID that was verified |
| `valid` | `bool` | `true` if the proof is correct |

#### Panics

Panics with `IpNotFound` (code 1) if any `ip_id` does not exist.

#### Example

```rust
let requests = vec![
    VerifyRequest { ip_id: 1, secret: s1, blinding_factor: b1 },
    VerifyRequest { ip_id: 2, secret: s2, blinding_factor: b2 },
];
let results = client.reveal_and_verify_commitments(&requests);
// results[0].valid == true/false
```

### `batch_verify_commitments`

Verify multiple IP commitments in a single call with a **real zero-knowledge proof**: each request supplies a `HidingCommitmentProof` — a non-interactive Fiat-Shamir Schnorr proof of knowledge of a Pedersen commitment's opening — and never the `secret` or `blinding_factor` itself. Only applies to IPs whose `commitment_hash` was committed as a Pedersen point (`secret·G + blinding_factor·H` over Ristretto255) rather than a SHA-256 hash; a request against a SHA-256-based commitment simply fails to verify (`valid: false`), it does not panic.

```rust
pub fn batch_verify_commitments(env: Env, requests: Vec<HidingVerifyRequest>) -> Vec<VerifyResult>
```

#### `HidingCommitmentProof`

| Field | Type | Description |
|---|---|---|
| `r` | `BytesN<32>` | Compressed Ristretto255 point `R = k_secret·G + k_blinding·H` |
| `s_secret` | `BytesN<32>` | Response scalar `s_secret = k_secret + e·secret` (mod L) |
| `s_blinding` | `BytesN<32>` | Response scalar `s_blinding = k_blinding + e·blinding_factor` (mod L) |

#### `HidingVerifyRequest`

| Field | Type | Description |
|---|---|---|
| `ip_id` | `u64` | The IP ID to verify |
| `proof` | `HidingCommitmentProof` | The zero-knowledge proof of the commitment's opening |

#### Panics

Panics with `IpNotFound` (code 1) if any `ip_id` does not exist. An invalid or malformed proof (or a non-Pedersen `commitment_hash`) does not panic — it yields `valid: false`.

#### Example

```rust
// Off-chain: compute commitment = secret·G + blinding_factor·H, and a
// proof of its opening (see docs/commitment-scheme.md). Neither secret
// nor blinding_factor is ever sent to the contract.
let requests = vec![
    HidingVerifyRequest { ip_id: 1, proof: proof1 },
    HidingVerifyRequest { ip_id: 2, proof: proof2 },
];
let results = client.batch_verify_commitments(&requests);
// results[0].valid == true/false
```

---

## Issue #462 — Batch Staking for IP Commitments

### `batch_stake_commitments`

Stake XLM against multiple IP commitments in a single contract call. Each `ip_ids[i]` is paired with `amounts[i]` and requires the corresponding IP owner to authorize the transaction.

```rust
pub fn batch_stake_commitments(env: Env, ip_ids: Vec<u64>, amounts: Vec<i128>)
```

#### Panics

- `BatchSizeMismatch` if `ip_ids.len() != amounts.len()`
- `AlreadyStaked` if any IP already has an active stake
- `StakeNotFound` is not returned by this call; it only operates on existing IPs

#### Example

```rust
let ip_ids = vec![1u64, 2u64];
let amounts = vec![100i128, 200i128];
client.batch_stake_commitments(&ip_ids, &amounts);
```

---

## Issue #463 — Batch Reputation Update for Multiple Commitments

### `batch_update_reputation`

Update reputation scores for multiple IP commitments in one call. Each `ip_ids[i]` is used to resolve the owner, and `score_deltas[i]` is applied to that owner's reputation record.

```rust
pub fn batch_update_reputation(env: Env, ip_ids: Vec<u64>, score_deltas: Vec<i64>)
```

#### Panics

- `BatchSizeMismatch` if `ip_ids.len() != score_deltas.len()`
- `Unauthorized` if the caller is not the configured admin
- `IpNotFound` if any IP ID does not exist

#### Example

```rust
let ip_ids = vec![1u64, 2u64];
let score_deltas = vec![10i64, -5i64];
client.batch_update_reputation(&ip_ids, &score_deltas);
```

---

## Issue #459 — Hierarchical Storage for Commitments

Organises IP commitments in a two-level hierarchy: `owner → category → ip_ids`. This enables O(1) category-scoped lookups without scanning the full owner index.

### Category Schema

Categories are human-readable hierarchical paths (e.g. `"Software/Cryptography/ZK-Proofs"`)
that are hashed with SHA-256 to produce a 32-byte category hash for on-chain storage.

**Constants:**

| Constant | Value | Description |
|---|---|---|
| `MAX_CATEGORY_DEPTH` | `10` | Maximum number of segments in a category path |

**Types:**

```rust
/// Metadata for a registered category path.
pub struct CategoryInfo {
    pub path: Bytes,  // full category path e.g. "Software/Cryptography/ZK-Proofs"
    pub depth: u32,   // number of segments (3 for the example above)
}
```

**Error Codes:**

| Error | Code | Description |
|---|---|---|
| `InvalidCategoryHash` | 32 | Category hash is all zeros |
| `InvalidCategoryDepth` | 33 | Category path exceeds `MAX_CATEGORY_DEPTH` |
| `CategoryNotFound` | 34 | Category hash not registered |

### `register_category_path`

Register a category path and compute its hash. Stores the depth for subsequent validation.

```rust
pub fn register_category_path(env: Env, path: Bytes) -> BytesN<32>
```

| Parameter | Type | Description |
|---|---|---|
| `path` | `Bytes` | Human-readable path, e.g. `b"Software/Cryptography/ZK-Proofs"` |

**Returns:** The SHA-256 hash of the path as `BytesN<32>`.

Panics with `InvalidCategoryDepth` if the path exceeds `MAX_CATEGORY_DEPTH` segments.

### `validate_category`

Validate that a category hash is registered and has a valid depth.

```rust
pub fn validate_category(env: Env, category_hash: BytesN<32>)
```

Panics with `InvalidCategoryHash` if the hash is all zeros, `CategoryNotFound` if the
category has not been registered, or `InvalidCategoryDepth` if the stored depth exceeds
the maximum.

### `assign_ip_to_category`

Assign an IP to a registered category within the owner's hierarchy. Only the IP owner
may call this. The category must have been previously registered via `register_category_path`.

```rust
pub fn assign_ip_to_category(env: Env, ip_id: u64, category_hash: BytesN<32>)
```

| Parameter | Type | Description |
|---|---|---|
| `ip_id` | `u64` | The IP to categorise |
| `category_hash` | `BytesN<32>` | 32-byte hash identifying the category (from `register_category_path`) |

Panics with `IpNotFound` if the IP does not exist, `InvalidCategoryHash` if the hash
is invalid, `CategoryNotFound` if unregistered, or auth error if caller is not the owner.
Duplicate assignments are silently ignored.

### `list_ip_by_category`

List all IP IDs for an owner within a specific category.

```rust
pub fn list_ip_by_category(env: Env, owner: Address, category_hash: BytesN<32>) -> Vec<u64>
```

Returns an empty vector if the owner has no IPs in that category.

### `list_owner_categories`

List all category hashes registered for an owner.

```rust
pub fn list_owner_categories(env: Env, owner: Address) -> Vec<BytesN<32>>
```

Returns an empty vector if the owner has no categories.

#### Example

```rust
// Register a hierarchical category
let path = Bytes::from_slice(&env, b"Software/Cryptography/ZK-Proofs");
let cat_hash = client.register_category_path(&path);

// Assign IP to category
client.assign_ip_to_category(&ip_id, &cat_hash);

// Query
let ids = client.list_ip_by_category(&owner, &cat_hash);
let cats = client.list_owner_categories(&owner);
```

---

## Input Validation Framework

All REST and GraphQL endpoints enforce comprehensive input validation to prevent injection attacks and ensure data consistency.

### Validation Principles

1. **OWASP Top 10 Compliance**: Protection against injection, XSS, and path traversal attacks
2. **Length Limits**: All strings limited to 10,000 characters; arrays to 1,000 items; addresses/URLs to 512 characters
3. **Null Byte Prevention**: Automatic detection and rejection of null byte injection (severity: HIGH)
4. **Type Safety**: Strong validation of data types and formats
5. **Composable Rules**: ValidationRules trait enables flexible, chainable validators

### Validation Error Response Format

All validation failures return HTTP 400 (Bad Request) with a standardized error response:

```json
{
  "error": "Request validation failed",
  "details": [
    {
      "field": "address",
      "message": "Invalid Stellar address format",
      "severity": "High"
    }
  ],
  "timestamp": "2024-01-15T10:30:45Z"
}
```

### Error Severity Levels

| Severity | Meaning | Action |
|---|---|---|
| `High` | Security or critical correctness issue | Request rejected immediately |
| `Medium` | Data quality issue | Request rejected, user should correct input |
| `Low` | Minor issue | Request rejected, typically informational |

### Field Validation Rules

#### Stellar Address (`address`)
- **Format**: Exactly 56 characters, starts with 'G'
- **Allowed Characters**: Base32 alphanumeric
- **Max Length**: 512 characters
- **Injection Protection**: Null bytes rejected
- **Examples**:
  - ✓ `GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZD`
  - ✗ `INVALID` (too short)
  - ✗ `0BRPYHIL...` (wrong first character)

#### Commitment Hash (`commitment_hash`, `hash`)
- **Format**: Hex-encoded 32-byte value (64 hex characters)
- **Allowed Characters**: 0-9, a-f, A-F
- **Max Length**: 10,000 characters
- **Examples**:
  - ✓ `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
  - ✗ `e3b0c442` (too short)
  - ✗ `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85g` (invalid hex)

#### Amount (`amount`, `price`, `balance`)
- **Type**: Positive 64-bit integer (>0)
- **Range**: 1 to 2^63 - 1
- **Examples**:
  - ✓ `1000`
  - ✗ `0` (must be positive)
  - ✗ `-100` (negative)

#### Timestamp (`timestamp`, `created_at`, `expires_at`)
- **Type**: Unix timestamp (seconds since epoch)
- **Valid Range**: 2000-01-01 to 2100-01-01
- **Examples**:
  - ✓ `1672531200` (2023-01-01)
  - ✗ `100` (too early)
  - ✗ `9999999999` (too far in future)

#### URL (`webhook_url`)
- **Format**: Must start with `http://` or `https://`
- **Max Length**: 512 characters
- **Injection Protection**: Null bytes rejected
- **Examples**:
  - ✓ `https://example.com/webhook`
  - ✗ `ftp://example.com` (unsupported protocol)
  - ✗ `example.com` (missing protocol)

#### Array Length (`ip_ids`, `prices`, `buyers`)
- **Min Length**: 1 item (cannot be empty)
- **Max Length**: 1,000 items
- **Validation**: No duplicate items allowed

#### String (`owner_name`, `description`)
- **Min Length**: 1 character (non-empty)
- **Max Length**: 10,000 characters
- **Injection Protection**: Null bytes rejected

### Composable Validation Example

Create custom validation rules by combining built-in validators:

```rust
use crate::validation::{
    ValidationRules, AddressValidationRule, AmountValidationRule,
};

let address_rule = AddressValidationRule {
    address: user_address.to_string(),
    field_name: "owner".to_string(),
};

let amount_rule = AmountValidationRule {
    amount: price,
    field_name: "price".to_string(),
};

// Chain rules together
let combined = address_rule.chain(Box::new(amount_rule));
match combined.validate() {
    Ok(_) => { /* proceed */ },
    Err(errors) => { /* handle errors */ },
}
```

### GraphQL Validation

GraphQL requests validate input at the resolver level. All scalar inputs use the same validation rules as REST endpoints.

Example mutation with validation:

```graphql
mutation {
  commitIp(input: {
    owner: "GBRPYHIL2CI3WHZDTOOQFC6EB4KJJGUJJBBQ5ECVVF7C3UFOCHJEAZD"
    commitmentHash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
  }) {
    ipId
    status
  }
}
```

### Security Considerations

1. **Null Byte Injection**: All string fields are checked for `\0` characters (OWASP injection prevention)
2. **Length Limits**: Prevents DoS attacks via excessively large payloads
3. **Type Coercion**: No automatic type conversion; strict type checking required
4. **Array Bounds**: Prevents memory exhaustion via large batch operations
5. **Timestamp Bounds**: Prevents logic errors from unrealistic timestamps

### Validation Middleware

A request-level middleware enforces validation before handlers execute:

```rust
.layer(middleware::from_fn(validation_middleware::validation_logging_middleware))
```

All validation failures are logged for monitoring and audit purposes.

---

## Rate limiting

The HTTP API uses token buckets to allow short bursts while recovering quota
continuously. Every request must have capacity in the global and source-IP
buckets. Authenticated requests must also have capacity in the bucket for the
JWT `sub` claim or API key. Exceeding any one of these quotas returns `429 Too Many
Requests`; rejected requests do not consume capacity from the other buckets.

### Default policy

| Scope or tier | Sustained rate | Burst capacity |
|---|---:|---:|
| Global (all clients) | 20,000 requests/minute | 2,000 |
| Source IP | 300 requests/minute | 100 |
| Free user | 60 requests/minute | 30 |
| Premium user | 600 requests/minute | 200 |
| Enterprise user | 6,000 requests/minute | 1,000 |

Unknown authenticated users receive the free tier. Premium and enterprise
assignments are made by trusted server-side billing/authentication code through
`RateLimitMiddleware::set_user_tier`; a request cannot select its own tier.
Operators can replace every quota through `RateLimitConfig` at startup.

Repeated violations apply exponential backoff beginning at one second and
capped at 60 seconds. A successful request clears the client's violation
streak. Idle client state expires after 15 minutes, and tracked IP/user counts
are bounded to prevent a distributed attack from exhausting limiter memory.

### Deployment topology

The quotas above are enforced correctly **only when every replica shares the
same counter backend**. `RateLimitConfig::backend` controls this:

- `RateLimitBackend::Redis(url)` — token-bucket counters live in Redis, shared
  by every replica pointed at that instance. **Required** behind a load
  balancer running more than one replica; otherwise each replica enforces its
  own independent bucket, and a client whose requests are distributed across
  N replicas effectively receives up to N times the quota documented above.
- `RateLimitBackend::InProcess` (the default) — counters live in that
  process's memory only. Safe for a single-instance deployment and for tests,
  but **not safe for multi-instance deployment** for the reason above.

Violation backoff tracking, tier assignment (`set_user_tier`), and the
tracked-identity cardinality bound stay process-local under both backends —
they are memory-protection heuristics for that replica, not part of the
enforced quota, so they don't need to be shared for the quota itself to be
correct.

### Response headers

All HTTP responses include:

| Header | Meaning |
|---|---|
| `X-RateLimit-Limit` | Burst capacity of the currently most constrained bucket |
| `X-RateLimit-Remaining` | Whole tokens remaining after this request |
| `X-RateLimit-Reset` | Unix timestamp when that bucket is full again |
| `X-RateLimit-Scope` | Constraining scope: `global`, `ip`, or `user` |
| `X-RateLimit-Tier` | Effective user tier (`free` for unauthenticated traffic) |

A `429` response also includes `Retry-After`, in seconds, and a JSON body:

```json
{
  "error": "rate_limit_exceeded",
  "scope": "user",
  "retry_after_seconds": 2
}
```

Clients should wait for `Retry-After` and add jitter before retrying. Requests
sent during a backoff window continue to receive `429`.

### Proxy deployment

The TCP peer address is used by default, and `X-Forwarded-For` is ignored to
prevent clients from evading IP limits by spoofing headers. Set
`RateLimitConfig::trust_proxy_headers` only when the API is reachable solely
through a trusted reverse proxy that replaces `X-Forwarded-For`.

---

## Idempotency

Write endpoints that accept an `x-idempotency-key` header (or an
`idempotency_key` body field, e.g. `POST /swap/batch-initiate`) deduplicate
retries: a request replayed with the same key within one hour of the
original returns the cached result of that original request — including its
HTTP status and body — instead of executing again, and the response carries
an `x-idempotency-replayed: true` header.

### Deployment topology

That guarantee is enforced correctly **only when every instance shares the
same deduplication backend**. `DeduplicationBackend` controls this for both
the completed-response cache (`DeduplicationStore`) and the in-flight marker
that stops a duplicate from executing concurrently with the original
(`ConcurrentDeduplicator`):

- `DeduplicationBackend::Redis(url)` — deduplication state lives in Redis,
  shared by every instance pointed at that instance/cluster. **Required**
  behind a load balancer running more than one instance; a client's retry has
  no guarantee of landing on the same instance that handled the original
  request, so without a shared backend the retry finds no record of the key
  on the instance that receives it and the request executes a second time —
  exactly the outcome an idempotency key exists to prevent. The batch-swap
  idempotency store (`handlers::BATCH_SWAP_IDEMPOTENCY`) picks this backend
  automatically when `REDIS_URL` is set.
- `DeduplicationBackend::InProcess` (the default) — state lives in that
  process's memory only. Safe for a single-instance deployment and for
  tests, but **not safe for multi-instance deployment** for the reason
  above: two instances each running this backend enforce independent state,
  not a shared one.

The one-hour TTL and the `x-idempotency-key` / `x-idempotency-replayed`
header contract are the same under both backends — only where the state
lives changes.
