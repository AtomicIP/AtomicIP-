# Partial Disclosure Proof Scheme

Partial disclosure lets an IP owner prove that their commitment covers a specific
design artifact (e.g. the SHA-256 of source code) **without revealing the full
secret or blinding factor**.

## Commitment Construction

When registering IP, the owner commits using:

```
commitment_hash = sha256(partial_hash || blinding_factor)
```

- `partial_hash` — `sha256` of the design artifact (source code, schematic, etc.)
- `blinding_factor` — a random 32-byte secret that hides `partial_hash` from observers
- `commitment_hash` — stored on-chain in the `IpRecord`

## Partial Reveal

To disclose the design artifact hash without revealing the full secret, the owner calls:

```rust
reveal_partial(env, ip_id, partial_hash, blinding_factor) -> bool
```

The contract verifies:

```
sha256(partial_hash || blinding_factor) == commitment_hash
```

If the proof is valid, `partial_hash` is stored publicly under `DataKey::PartialDisclosure(ip_id)`.

## Third-Party Verification

Anyone can retrieve the disclosed partial hash:

```rust
get_partial_disclosure(env, ip_id) -> Option<BytesN<32>>
```

A third party who independently computes `sha256(design_artifact)` can compare it
to the stored `partial_hash` to confirm the IP commitment covers that artifact.

## Security Properties

| Property | Guarantee |
|---|---|
| Binding | Owner cannot change `partial_hash` after committing — `commitment_hash` is fixed on-chain |
| Hiding | Without `blinding_factor`, observers cannot brute-force `partial_hash` from `commitment_hash` |
| Soundness | Only the holder of both `partial_hash` and `blinding_factor` can produce a valid proof |
| Auth | `reveal_partial` requires the owner's Soroban auth signature |

## Why Not Schnorr?

Soroban's host functions expose only `sha256` and `keccak256`. Schnorr signatures
require elliptic-curve scalar multiplication, which is not available as a host
function in Soroban SDK 22. The hash-based scheme above achieves equivalent
soundness for the partial-disclosure use case using only primitives available
on-chain.

## Example (Off-Chain Client)

```rust
// 1. Compute partial_hash from your design artifact
let partial_hash = sha256(source_code_bytes);

// 2. Choose a random blinding factor
let blinding_factor = random_bytes_32();

// 3. Compute commitment and register IP
let commitment_hash = sha256(partial_hash || blinding_factor);
let ip_id = client.commit_ip(&owner, &commitment_hash);

// 4. Later: partially disclose without revealing source code
client.reveal_partial(&ip_id, &partial_hash, &blinding_factor);

// 5. Third party verifies
let disclosed = client.get_partial_disclosure(&ip_id);
assert_eq!(disclosed, Some(sha256(their_copy_of_source_code)));
```
