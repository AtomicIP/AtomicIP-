# Pedersen Commitment Scheme

## Overview

AtomicIP uses a Pedersen commitment scheme to allow inventors to prove they held an idea at a specific time without revealing the idea itself. This document explains how to construct valid commitment hashes and secrets.

## How It Works

The commitment scheme uses SHA-256 hashing with a blinding factor to create a cryptographic commitment:

```
commitment_hash = sha256(secret || blinding_factor)
```

Where:
- `secret` - A 32-byte value representing your IP (e.g., a hash of your design document)
- `blinding_factor` - A 32-byte random value that hides the secret
- `||` - Concatenation operator
- `sha256` - The SHA-256 cryptographic hash function

## Secret Format

### What Constitutes a Valid Secret

A valid secret must be:

1. **Exactly 32 bytes** - The secret must be a `BytesN<32>` type
2. **Cryptographically random** - Use a secure random number generator
3. **Kept secret** - Only you should know the secret until you choose to reveal it
4. **Unique per commitment** - Each IP should have a different secret

### Recommended Secret Construction

For maximum security, construct your secret from your actual IP:

```rust
// Example: Creating a secret from a design document
use soroban_sdk::{BytesN, Env};

fn create_secret(env: &Env, design_document: &[u8]) -> BytesN<32> {
    // Hash the design document to create a 32-byte secret
    let secret: BytesN<32> = env.crypto().sha256(design_document).into();
    secret
}
```

### Alternative Secret Sources

You can use any 32-byte value as a secret:

- Hash of a PDF document
- Hash of source code
- Hash of a design schematic
- Randomly generated value (if you can remember it)

## Blinding Factor

The blinding factor is a random value that prevents attackers from guessing your secret through brute force.

### Generating a Secure Blinding Factor

```rust
use soroban_sdk::{BytesN, Env};

fn generate_blinding_factor(env: &Env) -> BytesN<32> {
    // Generate 32 random bytes
    let mut random_bytes = [0u8; 32];
    env.crypto().random_bytes(&mut random_bytes);
    BytesN::from_array(env, &random_bytes)
}
```

### Important Properties

- **Must be random** - Use cryptographically secure random generation
- **Must be kept secret** - Like the secret, the blinding factor must remain private
- **Must be unique** - Use a different blinding factor for each commitment

## Creating a Commitment Hash

### Complete Example

Here's a complete example showing how to create a commitment hash:

```rust
use soroban_sdk::{BytesN, Env};

/// Creates a Pedersen commitment hash from a secret and blinding factor.
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `secret` - The 32-byte secret representing your IP
/// * `blinding_factor` - The 32-byte random blinding factor
///
/// # Returns
///
/// The 32-byte commitment hash to register on-chain
///
/// # Example
///
/// ```ignore
/// let env = Env::default();
/// let secret = create_secret(&env, b"My invention design");
/// let blinding_factor = generate_blinding_factor(&env);
/// let commitment_hash = create_commitment_hash(&env, &secret, &blinding_factor);
/// ```
fn create_commitment_hash(
    env: &Env,
    secret: &BytesN<32>,
    blinding_factor: &BytesN<32>,
) -> BytesN<32> {
    // Concatenate secret || blinding_factor
    let mut preimage = soroban_sdk::Bytes::new(env);
    preimage.append(&secret.clone().into());
    preimage.append(&blinding_factor.clone().into());
    
    // Hash the preimage
    let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
    
    commitment_hash
}
```

### Step-by-Step Process

1. **Prepare your secret** - Hash your IP document or generate a random 32-byte value
2. **Generate blinding factor** - Create a random 32-byte value
3. **Concatenate** - Combine secret and blinding factor: `secret || blinding_factor`
4. **Hash** - Compute SHA-256 of the concatenated value
5. **Register** - Submit the commitment hash to the IP registry contract

## Verifying a Commitment

To verify a commitment, you need the original secret and blinding factor:

```rust
use soroban_sdk::BytesN;

/// Verifies that a secret and blinding factor match a commitment hash.
///
/// # Arguments
///
/// * `env` - The Soroban environment
/// * `commitment_hash` - The stored commitment hash to verify against
/// * `secret` - The secret to verify
/// * `blinding_factor` - The blinding factor to verify
///
/// # Returns
///
/// `true` if the secret and blinding factor produce the commitment hash
///
/// # Example
///
/// ```ignore
/// let is_valid = verify_commitment(
///     &env,
///     &stored_commitment_hash,
///     &secret,
///     &blinding_factor
/// );
/// ```
fn verify_commitment(
    env: &Env,
    commitment_hash: &BytesN<32>,
    secret: &BytesN<32>,
    blinding_factor: &BytesN<32>,
) -> bool {
    let computed_hash = create_commitment_hash(env, secret, blinding_factor);
    commitment_hash == &computed_hash
}
```

## Commitment Strength Scoring

Every IP commitment is assigned a **strength score** (0–100) that reflects the entropy and complexity of the commitment hash. Weak commitments (e.g. all-same-byte hashes or zero PoW) score low; strong, high-entropy commitments with meaningful PoW score near 100.

### Scoring Formula

```
entropy_score = (unique_bytes_in_hash * 50) / 32   // 0–50 points
pow_score     = min(50, (pow_difficulty * 50) / 32) // 0–50 points
strength      = min(100, entropy_score + pow_score)
```

| Component | Max Points | Description |
|-----------|-----------|-------------|
| Byte entropy | 50 | Number of unique byte values in the 32-byte commitment hash, scaled to 0–50 |
| PoW difficulty | 50 | Leading-zero-bit difficulty used at commit time, scaled to 0–50 (32 bits = 50 pts) |

### Querying Strength

```rust
let strength: u32 = registry.get_ip_strength(&ip_id);
// Returns 0–100
```

### Practical Guidance

- A SHA-256 hash of real content will have ~30–32 unique bytes → ~47–50 entropy points.
- Using `pow_difficulty = 4` (default) adds ~6 points.
- A typical real-world commitment scores **53–56 / 100**.
- To reach 100, use a high-entropy hash (32 unique bytes) with `pow_difficulty ≥ 32`.

### Why Entropy Matters

A commitment hash derived from a real design document (via SHA-256) will have high byte entropy — the 256 possible byte values are roughly uniformly distributed. A weak hash like `[0x01; 32]` (all same byte) signals the commitment may not represent genuine IP, and scores near zero.



### Why Use a Blinding Factor?

Without a blinding factor, an attacker could:
1. Guess common secrets (e.g., "patent application 2024")
2. Hash the guess
3. Compare against all commitment hashes
4. Identify which commitments match their guess

The blinding factor makes this attack computationally infeasible.

### Secret Storage

**CRITICAL**: If you lose your secret and blinding factor, you cannot:
- Prove ownership of your IP
- Complete an atomic swap
- Reveal your IP to buyers

Store your secret and blinding factor securely:
- Use encrypted storage
- Create multiple backups
- Store in different physical locations
- Never share until you're ready to reveal

### What Happens If Your Secret Is Leaked?

If someone discovers your secret before you reveal it:
- They can claim they own the IP (but cannot prove it on-chain without your signature)
- They cannot complete a swap (they need your authorization)
- You should still be able to prove ownership via your Stellar wallet signature

## Common Mistakes to Avoid

### ❌ Using the Same Secret for Multiple IPs

```rust
// WRONG - Don't do this!
let secret = BytesN::from_array(&env, &[1u8; 32]);
let hash1 = create_commitment_hash(&env, &secret, &blinding_factor1);
let hash2 = create_commitment_hash(&env, &secret, &blinding_factor2);
// If someone discovers the secret, they can claim both IPs
```

### ❌ Using Predictable Blinding Factors

```rust
// WRONG - Don't do this!
let blinding_factor = BytesN::from_array(&env, &[0u8; 32]); // All zeros
// Attackers can easily guess this
```

### ❌ Not Storing the Secret

```rust
// WRONG - Don't do this!
let secret = generate_random_secret();
let commitment_hash = create_commitment_hash(&env, &secret, &blinding_factor);
// If you don't store the secret, you can never prove ownership!
```

### ✅ Correct Approach

```rust
// CORRECT - Do this!
let secret = create_secret(&env, my_design_document);
let blinding_factor = generate_blinding_factor(&env);
let commitment_hash = create_commitment_hash(&env, &secret, &blinding_factor);

// Store both securely!
store_secret_securely(&secret);
store_blinding_factor_securely(&blinding_factor);
```

## Complete Workflow Example

Here's a complete workflow for registering and verifying IP:

```rust
use soroban_sdk::{BytesN, Env, Address};

/// Complete workflow for registering IP with a Pedersen commitment
fn register_ip_workflow(env: &Env, owner: &Address, design_document: &[u8]) {
    // 1. Create secret from design document
    let secret = create_secret(env, design_document);
    
    // 2. Generate random blinding factor
    let blinding_factor = generate_blinding_factor(env);
    
    // 3. Create commitment hash
    let commitment_hash = create_commitment_hash(env, &secret, &blinding_factor);
    
    // 4. Register on-chain (this is done via the contract)
    // let ip_id = registry.commit_ip(owner, &commitment_hash);
    
    // 5. Store secret and blinding factor securely OFF-CHAIN
    // This is your responsibility - the blockchain doesn't store these!
    store_offchain(&secret, &blinding_factor);
}

/// Later, to verify or complete a swap:
fn verify_ip_workflow(env: &Env, commitment_hash: &BytesN<32>) -> bool {
    // 1. Retrieve your secret and blinding factor from secure storage
    let (secret, blinding_factor) = retrieve_from_secure_storage();
    
    // 2. Verify they match the commitment
    verify_commitment(env, commitment_hash, &secret, &blinding_factor)
}
```

## Technical Details

### Why SHA-256?

AtomicIP uses SHA-256 because:
- It's cryptographically secure
- It's widely supported in Soroban
- It produces fixed-size 32-byte outputs
- It's resistant to collision attacks

### Why Not True Pedersen Commitments?

True Pedersen commitments use elliptic curve cryptography and have special properties:
- Homomorphic: `C(m1) * C(m2) = C(m1 + m2)`
- Perfectly hiding: Commitment reveals nothing about the message
- Computationally binding: Cannot change the message after committing

AtomicIP uses a simpler SHA-256-based scheme because:
- It's easier to implement and verify
- It's sufficient for the use case (proving prior art)
- It has lower gas costs
- It's more accessible to developers

The trade-off is that SHA-256 commitments are not homomorphic, but this property isn't needed for IP registration.

## Batch Verification: Reveal-and-Compare (Issue #458)

AtomicIP supports **batch verification** that checks multiple commitments in a single on-chain
call and folds the results into a deterministic aggregate proof.

There are two batch entry points, named for exactly what they disclose:

| Function | Discloses `secret`/`blinding_factor`? | Use when |
|---|---|---|
| [`reveal_and_verify_commitments`](#function-signature) | **Yes** — plaintext, in the call arguments | The caller intends to disclose (e.g. publishing prior art) |
| [`batch_verify_commitments`](#batch-verification-zero-knowledge-hiding-proof-issue-780) | **No** — a zero-knowledge proof only | The caller wants to prove knowledge without revealing anything |

This section covers `reveal_and_verify_commitments`. See
[Batch Verification: Zero-Knowledge Hiding Proof](#batch-verification-zero-knowledge-hiding-proof-issue-780)
below for the ZK path.

### How It Works

A single call to `reveal_and_verify_commitments` processes N verification requests and produces:

- A `Vec<VerifyResult>` — one result per request in input order
- An **aggregate proof hash** — a single 32-byte value that cryptographically binds all validated commitments

1. **Individual verification**: for each request, the contract loads the `IpRecord` for `ip_id`,
   computes `sha256(secret || blinding_factor)`, and compares it against the stored
   `commitment_hash` using **constant-time comparison** (`constant_time_bytes_32_eq`).
2. **Aggregate proof construction**: valid commitment hashes are folded into a single proof using
   **incremental SHA-256 hashing**, skipping invalid entries entirely:

   ```
   proof_0 = 0x0000...0000          (32 zero bytes)
   proof_1 = sha256(proof_0 || hash_1)   # only if verification 1 is valid
   proof_2 = sha256(proof_1 || hash_2)   # only if verification 2 is valid
   ...
   proof_N = sha256(proof_{N-1} || hash_N)
   ```

   This produces a deterministic, order-dependent proof: the same requests in a different order
   yield a different aggregate proof, preventing replay across reordered batches.
3. **Storage**: the aggregate proof and summary counts are stored under
   `DataKey::BatchVerifyResult(aggregate_proof)`.
4. **Event emission**: a `batch_vfy` event is published with the aggregate proof and counts.

### Function Signature

```rust
pub fn reveal_and_verify_commitments(
    env: Env,
    requests: Vec<VerifyRequest>,
) -> Vec<VerifyResult>

/// Retrieve a stored batch verification proof by its aggregate hash.
pub fn verify_batch_proof(
    env: Env,
    proof_hash: BytesN<32>,
) -> Option<BatchVerifyResultStorage>
```

#### `VerifyRequest`

| Field | Type | Description |
|---|---|---|
| `ip_id` | `u64` | The IP ID to verify |
| `secret` | `BytesN<32>` | The secret used when committing — **disclosed on-chain** |
| `blinding_factor` | `BytesN<32>` | The blinding factor used when committing — **disclosed on-chain** |

#### `VerifyResult`

| Field | Type | Description |
|---|---|---|
| `ip_id` | `u64` | The IP ID that was verified |
| `valid` | `bool` | `true` if the proof is correct |

#### `BatchVerifyResultStorage`

| Field | Type | Description |
|---|---|---|
| `aggregate_proof` | `BytesN<32>` | The aggregate proof hash (also the storage key) |
| `total_count` | `u32` | Number of requests in the batch |
| `valid_count` | `u32` | Number of requests that verified successfully |

### Event: `batch_vfy`

```
topic:  (symbol_short!("b_vfy"),)
data:   (BytesN<32>, u32, u32)  // (aggregate_proof, total_count, valid_count)
```

Off-chain listeners can subscribe to this event to track batch verification completion without
replaying individual checks.

### Security: Constant-Time Comparison

```rust
fn constant_time_bytes_32_eq(a: &BytesN<32>, b: &BytesN<32>) -> bool {
    let a_arr = a.to_array();
    let b_arr = b.to_array();
    let mut diff: u8 = 0;
    for i in 0..32 {
        diff |= a_arr[i] ^ b_arr[i];
    }
    diff == 0
}
```

This XORs every byte pair and ORs the results together. Unlike `==`, it never short-circuits and
always touches all 32 bytes, preventing timing side-channel attacks.

### Edge Cases

| Scenario | Behaviour |
|----------|-----------|
| **Empty batch** | Returns an empty `Vec<VerifyResult>`, aggregate proof is `sha256(0x00..00)`, event emitted with `total_count=0, valid_count=0` |
| **Single item** | Returns one `VerifyResult`; aggregate proof equals `sha256(0x00..00 || hash)` if valid, or remains the zero seed if invalid |
| **Non-existent IP** | Panics with `IpNotFound` — a missing IP is a fatal error, not an invalid result |
| **All invalid** | Aggregate proof remains `0x00..00` (the seed) |
| **Mixed valid/invalid** | Only valid hashes contribute to the aggregate proof; invalid entries are skipped |

### Example

```rust
use soroban_sdk::{BytesN, Vec};

let mut requests = Vec::new(&env);
requests.push_back(VerifyRequest { ip_id: 1, secret: secret1, blinding_factor: blind1 });
requests.push_back(VerifyRequest { ip_id: 2, secret: secret2, blinding_factor: blind2 });

let results: Vec<VerifyResult> = client.reveal_and_verify_commitments(&requests);

for r in results.iter() {
    // r.ip_id, r.valid
}

// The aggregated proof is stored on-chain and retrievable by its hash:
// let proof = client.verify_batch_proof(&aggregate_proof);
```

## Batch Verification: Zero-Knowledge Hiding Proof (Issue #780)

`batch_verify_commitments` is the genuinely zero-knowledge counterpart to
`reveal_and_verify_commitments` above: it verifies knowledge of a commitment's opening **without
the caller ever placing `secret` or `blinding_factor` in a transaction argument, event, or storage
entry.**

### Why this exists

Earlier revisions of this document (and of `docs/api-reference.md`) described
`batch_verify_commitments` as having "ZK Proof Support" while its actual behavior was the plaintext
reveal-and-compare now named `reveal_and_verify_commitments` above — the same SHA-256 scheme
described in [Why Not True Pedersen Commitments?](#why-not-true-pedersen-commitments), which is
*not* zero-knowledge by design. `batch_verify_commitments` now does what the old docs claimed:
a real hiding proof.

### Commitment format

This path only applies to IPs whose `commitment_hash` was created as a genuine **Pedersen
commitment over Ristretto255** — `commitment = secret·G + blinding_factor·H` — rather than a
SHA-256 hash. `commit_ip` already stores `commitment_hash` as an opaque 32-byte value, so no
contract storage change was needed: a caller who wants hiding verification simply computes the
Pedersen point off-chain and passes its compressed 32-byte encoding to `commit_ip` instead of a
SHA-256 hash. `secret`/`blinding_factor` are interpreted as Ristretto255 scalars (reduced mod the
group order), not as raw hash preimage bytes — this is a different encoding from the
`reveal_and_verify_commitments` path above, so the two are not interchangeable for the same IP.

`G` is the standard Ristretto255 basepoint. `H` is a second, independent "nothing up my sleeve"
generator with no known discrete log relative to `G`:

```
H = ristretto255_hash_to_group(SHA512("AtomicIP/PedersenCommitment/H/v1"))
```

Anyone can independently recompute `H` — see `contracts/ip_registry/src/zk_commitment.rs` for the
exact derivation and the resulting constant.

### The proof: Fiat-Shamir Schnorr

The proof is a standard Okamoto/Schnorr proof of knowledge of a representation, made
non-interactive via Fiat-Shamir:

**Prover** (off-chain), given `(secret, blinding_factor)` and fresh random nonces
`(k_secret, k_blinding)`:

1. `R = k_secret·G + k_blinding·H`
2. `e = sha256("AtomicIP/HidingCommitmentProof/v1" || commitment || R) mod L` (the Fiat-Shamir challenge)
3. `s_secret = k_secret + e·secret`, `s_blinding = k_blinding + e·blinding_factor` (mod L)
4. Submit `HidingCommitmentProof { r: R, s_secret, s_blinding }`

**Verifier** (on-chain), given `commitment` and the proof:

- Recomputes `e` the same way
- Accepts iff `s_secret·G + s_blinding·H == R + e·commitment`

Neither `secret` nor `blinding_factor` ever appears in the proof, an argument, an event, or
storage. As with any Schnorr-style proof, **a nonce must never be reused across two different
proofs** — nonce generation is the prover's responsibility and happens entirely off-chain; the
contract only verifies.

### Function Signature

```rust
pub fn batch_verify_commitments(
    env: Env,
    requests: Vec<HidingVerifyRequest>,
) -> Vec<VerifyResult>
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

`VerifyResult`, the `batch_vfy` event, aggregate-proof construction, and storage under
`DataKey::BatchVerifyResult` all work identically to `reveal_and_verify_commitments` above — the
only difference is how each individual request is verified.

### Failure modes

| Scenario | Behaviour |
|---|---|
| `commitment_hash` or `proof.r` is not a valid Ristretto255 point encoding (e.g. it's a SHA-256 hash, not a Pedersen point) | `valid: false` — never panics |
| Proof built from the wrong `secret`/`blinding_factor` | `valid: false` |
| Non-existent `ip_id` | Panics with `IpNotFound`, same as `reveal_and_verify_commitments` |

## References

- [SHA-256 Wikipedia](https://en.wikipedia.org/wiki/SHA-2)
- [Pedersen Commitment Wikipedia](https://en.wikipedia.org/wiki/Pedersen_commitment)
- [Schnorr Signature / Sigma Protocols Wikipedia](https://en.wikipedia.org/wiki/Schnorr_signature)
- [Fiat–Shamir Heuristic Wikipedia](https://en.wikipedia.org/wiki/Fiat%E2%80%93Shamir_heuristic)
- [Ristretto Group](https://ristretto.group/)
- [Soroban Cryptography Documentation](https://soroban.stellar.org/docs/reference/environment-functions/crypto)
- [NIST SHA-2 Standard](https://csrc.nist.gov/publications/detail/fips/180-4/final)

## Questions?

If you have questions about the commitment scheme:
- Open a [GitHub Issue](https://github.com/AtomicIP/AtomicIP-/issues)
- Join our [Discord community](https://discord.gg/atomicip)
- Email: support@atomicip.io

## Commitment Renewal

IP commitments have an on-chain TTL of approximately 1 year (~6,307,200 ledgers). Owners can renew
an expiring commitment without re-committing or changing the commitment hash:

```rust
fn renew_ip(ip_id: u64)
```

- Requires owner authorization
- Resets the storage TTL back to `LEDGER_BUMP` (~1 year)
- Increments an on-chain renewal counter (queryable via `get_renewal_count`)
- Emits a `renewed` event with `(ip_id, renewal_count)`
- Panics if the IP is revoked or does not exist

The original commitment hash, timestamp, and owner are **never modified** by renewal — prior art
proof is fully preserved.

```rust
fn get_renewal_count(ip_id: u64) -> u32
```

Returns how many times the IP has been renewed (0 if never renewed).
