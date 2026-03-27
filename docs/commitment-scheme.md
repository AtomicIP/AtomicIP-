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

## Security Considerations

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

## References

- [SHA-256 Wikipedia](https://en.wikipedia.org/wiki/SHA-2)
- [Pedersen Commitment Wikipedia](https://en.wikipedia.org/wiki/Pedersen_commitment)
- [Soroban Cryptography Documentation](https://soroban.stellar.org/docs/reference/environment-functions/crypto)
- [NIST SHA-2 Standard](https://csrc.nist.gov/publications/detail/fips/180-4/final)

## Questions?

If you have questions about the commitment scheme:
- Open a [GitHub Issue](https://github.com/AtomicIP/AtomicIP-/issues)
- Join our [Discord community](https://discord.gg/atomicip)
- Email: support@atomicip.io
