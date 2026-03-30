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

### Example

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

### Example

```rust
let hashes = Vec::from_array(&env, [hash1, hash2, hash3]);
let ip_ids = registry.batch_commit_ip(&owner, &hashes);
// ip_ids = [1, 2, 3]
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

### Example

```rust
let record = registry.get_ip(&ip_id);
println!("Owner: {}", record.owner);
println!("Timestamp: {}", record.timestamp);
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

### Example

```rust
let is_valid = registry.verify_commitment(&ip_id, &secret, &blinding_factor);
if is_valid {
    println!("Commitment verified!");
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

### Example

```rust
let ip_ids = registry.list_ip_by_owner(&owner);
for ip_id in ip_ids.iter() {
    let record = registry.get_ip(&ip_id);
    println!("IP {}: {}", ip_id, record.commitment_hash);
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

### Example

```rust
registry.transfer_ip(&ip_id, &new_owner);
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

### Example

```rust
registry.revoke_ip(&ip_id);
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
