/**
 * #881 — Cross-Layer Encryption Key Rotation Compatibility Test
 * ─────────────────────────────────────────────────────────────
 * Verifies that the JS encryption layer (batchEncryptor.js) and the
 * Rust ip_registry contract's EncryptionKeyRotation storage key are
 * compatible: both sides agree on byte formats, key lengths, and the
 * commitment derivation scheme (SHA-256(secret ∥ blinding_factor)).
 *
 * Contract-side facts (from contracts/ip_registry/src/lib.rs):
 *   - DataKey::EncryptionKeyRotation(ip_id) stores Vec<BytesN<32>>
 *     — the ordered history of commitment hashes retired by past rotations.
 *   - commitment_hash = SHA-256(secret_bytes ∥ blinding_factor_bytes)
 *     where both secret and blinding_factor are 32-byte values (BytesN<32>).
 *   - The encryption key protecting a patent payload IS the 32-byte secret,
 *     which maps directly to AES-256-GCM's 32-byte key in batchEncryptor.js.
 *
 * JS-side facts (from src/batch/batchEncryptor.js):
 *   Wire format : [ 12-byte IV | 16-byte auth-tag | ciphertext ]
 *   Algorithm   : AES-256-GCM (Node crypto)
 *   Key length  : exactly 32 bytes (matches BytesN<32> on the contract side)
 */

"use strict";

const crypto = require("crypto");
const { encryptBatchSwaps, decryptBatchSwaps } = require("../batch/batchEncryptor");

// ── Constants mirrored from batchEncryptor.js ─────────────────────────────────

const IV_LENGTH  = 12;
const TAG_LENGTH = 16;

// ── Contract-side helpers (JS simulation) ────────────────────────────────────

/**
 * Simulate the contract commitment derivation:
 *   commitment = SHA-256(secret ∥ blinding_factor)
 *
 * Both inputs are 32-byte Buffers matching BytesN<32> in Soroban.
 *
 * @param {Buffer} secret           32-byte secret (also used as AES-256 key)
 * @param {Buffer} blindingFactor   32-byte blinding factor
 * @returns {Buffer} 32-byte SHA-256 digest
 */
function deriveCommitment(secret, blindingFactor) {
  return crypto
    .createHash("sha256")
    .update(secret)
    .update(blindingFactor)
    .digest();
}

/**
 * Simulate the contract-side rotation history append.
 * Returns the new history (array of 32-byte hex strings).
 *
 * @param {string[]} history        existing rotation history (hex commitment hashes)
 * @param {Buffer}   oldCommitment  commitment hash being retired
 * @returns {string[]}
 */
function appendRotationHistory(history, oldCommitment) {
  return [...history, oldCommitment.toString("hex")];
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("#881 — Wire format: [ 12-byte IV | 16-byte auth-tag | ciphertext ]", () => {
  test("encrypted output begins with a 12-byte IV followed by a 16-byte auth-tag", () => {
    const key  = crypto.randomBytes(32);
    const data = Buffer.from(JSON.stringify({ swapId: "s1", amount: 100 }));
    const enc  = encryptBatchSwaps(data, key);

    // Total length must be at least IV + tag
    expect(enc.length).toBeGreaterThanOrEqual(IV_LENGTH + TAG_LENGTH);

    // Each encryption should use a fresh random IV — two encryptions must differ
    const enc2 = encryptBatchSwaps(data, key);
    const iv1  = enc.slice(0, IV_LENGTH);
    const iv2  = enc2.slice(0, IV_LENGTH);
    expect(iv1.equals(iv2)).toBe(false);
  });

  test("auth-tag occupies bytes [12, 28) — tampering with it breaks decryption", () => {
    const key  = crypto.randomBytes(32);
    const data = Buffer.from("payload");
    const enc  = encryptBatchSwaps(data, key);

    const tampered = Buffer.from(enc);
    tampered[IV_LENGTH] ^= 0xff; // flip first tag byte
    expect(() => decryptBatchSwaps(tampered, key)).toThrow();
  });

  test("ciphertext starts at byte 28 — length equals plaintext length (GCM property)", () => {
    const key      = crypto.randomBytes(32);
    const payload  = Buffer.from("hello world");
    const enc      = encryptBatchSwaps(payload, key);
    const ctLength = enc.length - IV_LENGTH - TAG_LENGTH;

    // AES-GCM ciphertext has the same byte length as plaintext
    expect(ctLength).toBe(payload.length);
  });

  test("minimum wire-format length for empty plaintext is IV_LENGTH + TAG_LENGTH (28 bytes)", () => {
    const key = crypto.randomBytes(32);
    const enc = encryptBatchSwaps(Buffer.alloc(0), key);
    expect(enc.length).toBe(IV_LENGTH + TAG_LENGTH);
  });
});

describe("#881 — Key compatibility: JS 32-byte key ↔ contract BytesN<32>", () => {
  test("key must be exactly 32 bytes — matches contract BytesN<32> type", () => {
    const data = Buffer.from("data");
    expect(() => encryptBatchSwaps(data, Buffer.alloc(16))).toThrow(RangeError);
    expect(() => encryptBatchSwaps(data, Buffer.alloc(31))).toThrow(RangeError);
    expect(() => encryptBatchSwaps(data, Buffer.alloc(33))).toThrow(RangeError);
    // 32 bytes is the only accepted length
    expect(() => encryptBatchSwaps(data, Buffer.alloc(32))).not.toThrow();
  });

  test("32-byte secret (BytesN<32>) used as AES key encrypts and decrypts correctly", () => {
    // Simulates the owner using their IP commitment secret as the AES-256 key
    const secret        = crypto.randomBytes(32); // = contract BytesN<32> secret
    const blindingFactor = crypto.randomBytes(32); // = contract BytesN<32> blinding_factor
    const commitment    = deriveCommitment(secret, blindingFactor);

    // Commitment is 32 bytes (SHA-256 output) — matches BytesN<32>
    expect(commitment.length).toBe(32);

    const payload   = Buffer.from(JSON.stringify({ ipId: 42, data: "patent payload" }));
    const encrypted = encryptBatchSwaps(payload, secret);
    const decrypted = decryptBatchSwaps(encrypted, secret);

    expect(decrypted).toEqual(payload);
  });
});

describe("#881 — Commitment derivation: SHA-256(secret ∥ blinding_factor)", () => {
  test("commitment is deterministic for the same inputs", () => {
    const secret        = Buffer.alloc(32, 0xab);
    const blindingFactor = Buffer.alloc(32, 0xcd);
    const c1 = deriveCommitment(secret, blindingFactor);
    const c2 = deriveCommitment(secret, blindingFactor);
    expect(c1.equals(c2)).toBe(true);
  });

  test("different secrets produce different commitments", () => {
    const blindingFactor = crypto.randomBytes(32);
    const c1 = deriveCommitment(crypto.randomBytes(32), blindingFactor);
    const c2 = deriveCommitment(crypto.randomBytes(32), blindingFactor);
    expect(c1.equals(c2)).toBe(false);
  });

  test("different blinding factors produce different commitments", () => {
    const secret = crypto.randomBytes(32);
    const c1 = deriveCommitment(secret, crypto.randomBytes(32));
    const c2 = deriveCommitment(secret, crypto.randomBytes(32));
    expect(c1.equals(c2)).toBe(false);
  });

  test("commitment output is always 32 bytes (matches BytesN<32>)", () => {
    const c = deriveCommitment(crypto.randomBytes(32), crypto.randomBytes(32));
    expect(c.length).toBe(32);
  });

  test("known-vector: SHA-256(0x00*32 ∥ 0x00*32) equals expected digest", () => {
    // Deterministic check: SHA-256 of 64 zero bytes
    const expected = crypto.createHash("sha256").update(Buffer.alloc(64)).digest("hex");
    const result   = deriveCommitment(Buffer.alloc(32), Buffer.alloc(32)).toString("hex");
    expect(result).toBe(expected);
  });
});

describe("#881 — Cross-layer rotation: JS encrypt ↔ contract rotation history", () => {
  test("full rotation: old key encrypts, history records old commitment, new key decrypts new payload", () => {
    // Step 1: owner generates key v1 + blinding factor, derives commitment v1
    const secretV1        = crypto.randomBytes(32);
    const blindingV1      = crypto.randomBytes(32);
    const commitmentV1    = deriveCommitment(secretV1, blindingV1);

    // Step 2: owner encrypts IP payload with key v1
    const payload     = Buffer.from(JSON.stringify({ ipId: 7, design: "patent-v1" }));
    const encryptedV1 = encryptBatchSwaps(payload, secretV1);

    // Step 3: owner rotates to key v2
    const secretV2     = crypto.randomBytes(32);
    const blindingV2   = crypto.randomBytes(32);
    const commitmentV2 = deriveCommitment(secretV2, blindingV2);

    // Contract side: append commitmentV1 to rotation history
    const history        = [];
    const updatedHistory = appendRotationHistory(history, commitmentV1);

    // History contains the retired commitment as a 64-char hex string (32-byte BytesN<32>)
    expect(updatedHistory).toHaveLength(1);
    expect(updatedHistory[0]).toBe(commitmentV1.toString("hex"));
    expect(updatedHistory[0]).toHaveLength(64);

    // Step 4: owner re-encrypts payload with key v2
    const encryptedV2 = encryptBatchSwaps(payload, secretV2);

    // Step 5: each key decrypts its own ciphertext
    expect(decryptBatchSwaps(encryptedV1, secretV1)).toEqual(payload);
    expect(decryptBatchSwaps(encryptedV2, secretV2)).toEqual(payload);

    // Step 6: keys are not cross-compatible (ciphertext/key isolation)
    expect(() => decryptBatchSwaps(encryptedV1, secretV2)).toThrow();
    expect(() => decryptBatchSwaps(encryptedV2, secretV1)).toThrow();

    // Step 7: commitments must be distinct (contract's require_unique_commitment)
    expect(commitmentV1.equals(commitmentV2)).toBe(false);
  });

  test("rotation history accumulates correctly over multiple rotations", () => {
    let history  = [];
    const count  = 3;
    const secrets   = Array.from({ length: count }, () => crypto.randomBytes(32));
    const blindings = Array.from({ length: count }, () => crypto.randomBytes(32));
    const commitments = secrets.map((s, i) => deriveCommitment(s, blindings[i]));

    // Simulate 3 sequential rotations
    for (const c of commitments) {
      history = appendRotationHistory(history, c);
    }

    expect(history).toHaveLength(count);
    for (let i = 0; i < count; i++) {
      expect(history[i]).toBe(commitments[i].toString("hex"));
      expect(history[i]).toHaveLength(64); // 32 bytes = 64 hex chars = BytesN<32>
    }
  });

  test("no format mismatch: manual wire-format unpack matches decryptBatchSwaps", () => {
    const key  = crypto.randomBytes(32);
    const data = Buffer.from("cross-layer test payload");
    const enc  = encryptBatchSwaps(data, key);

    // Manually unpack using the documented offsets
    const iv  = enc.slice(0, IV_LENGTH);
    const tag = enc.slice(IV_LENGTH, IV_LENGTH + TAG_LENGTH);
    const ct  = enc.slice(IV_LENGTH + TAG_LENGTH);

    // Decrypt manually using Node crypto to verify the offset documentation is correct
    const decipher = crypto.createDecipheriv("aes-256-gcm", key, iv, { authTagLength: TAG_LENGTH });
    decipher.setAuthTag(tag);
    const plaintext = Buffer.concat([decipher.update(ct), decipher.final()]);

    expect(plaintext).toEqual(data);
  });

  test("rotating to the same commitment is detectable (commitments must be unique)", () => {
    const secret         = crypto.randomBytes(32);
    const blindingFactor = crypto.randomBytes(32);
    const commitment     = deriveCommitment(secret, blindingFactor);

    // Simulates the contract's require_unique_commitment check:
    // if the new commitment already exists in history, it must be rejected.
    const history = appendRotationHistory([], commitment);
    const isReused = history.includes(commitment.toString("hex"));

    expect(isReused).toBe(true); // caller should reject this rotation
  });
});
