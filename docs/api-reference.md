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
| `InvalidCategoryHash` | 29 | Category hash is all zeros or malformed |
| `CategoryDepthExceeded` | 30 | Category path exceeds 5 levels |
| `CategoryPathTraversal` | 31 | Category path contains traversal or empty segments |

---

## Events

### `ip_commit`

Emitted when a new IP is committed.

**Topics:** `(symbol_short!("ip_commit"), owner: Address)`  
**Data:** `(ip_id: u64, timestamp: u64)`

---

### `ip_cat`

Emitted when an IP is assigned to a category (Issue #459).

**Topics:** `(symbol_short!("ip_cat"), owner: Address)`  
**Data:** `(ip_id: u64, category_hash: BytesN<32>)`

---

## Storage Keys

| Key | Type | Description |
|---|---|---|
| `IpRecord(u64)` | Persistent | Stores IP record by ID |
| `OwnerIps(Address)` | Persistent | Maps owner → Vec of IP IDs |
| `NextId` | Persistent | Next available IP ID (monotonic counter) |
| `CommitmentOwner(BytesN<32>)` | Persistent | Maps commitment hash → owner (duplicate detection) |
| `Admin` | Persistent | Admin address for upgrades |
| `CategoryIps(BytesN<32>)` | Persistent | Maps category hash → Vec of IP IDs |
| `OwnerCategories(Address)` | Persistent | Maps owner → Vec of category hashes they use |
| `IpCategories(u64)` | Persistent | Maps IP ID → Vec of assigned category hashes |

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

## Issue #458 — Batch Verification with ZK Proofs

### `batch_verify_commitments`

Verify multiple IP commitments in a single call. Each request recomputes `sha256(secret || blinding_factor)` and checks it against the stored commitment hash — the same zero-knowledge proof used by `verify_commitment`.

```rust
pub fn batch_verify_commitments(env: Env, requests: Vec<VerifyRequest>) -> Vec<VerifyResult>
```

#### `VerifyRequest`

| Field | Type | Description |
|---|---|---|
| `ip_id` | `u64` | The IP ID to verify |
| `secret` | `BytesN<32>` | The secret used when committing |
| `blinding_factor` | `BytesN<32>` | The blinding factor used when committing |

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

### `assign_ip_to_category`

Assign an IP to a category within the owner's hierarchy. Only the IP owner may call this.

```rust
pub fn assign_ip_to_category(env: Env, ip_id: u64, category_hash: BytesN<32>)
```

| Parameter | Type | Description |
|---|---|---|
| `ip_id` | `u64` | The IP to categorise |
| `category_hash` | `BytesN<32>` | 32-byte hash identifying the category (e.g. `sha256(label)`) |

Panics with `IpNotFound` if the IP does not exist, or auth error if caller is not the owner. Duplicate assignments are silently ignored.

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
let category = env.crypto().sha256(&Bytes::from_slice(&env, b"patents"));
client.assign_ip_to_category(&ip_id, &category);

let ids = client.list_ip_by_category(&owner, &category);
let cats = client.list_owner_categories(&owner);
```
