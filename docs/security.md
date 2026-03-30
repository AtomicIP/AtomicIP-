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
