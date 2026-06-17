/**
 * Swap Batch Encryption (#526)
 * ────────────────────────────
 * Off-chain utility to encrypt / decrypt arrays of swap records using
 * AES-256-GCM via Node's built-in `crypto` module.
 *
 * The wire format is:  [ 12-byte IV | 16-byte auth-tag | ciphertext ]
 * Authentication is built into GCM, so tampered ciphertext is rejected
 * automatically during decryption.
 */

const crypto = require("crypto");

const ALGORITHM  = "aes-256-gcm";
const IV_LENGTH  = 12; // 96-bit IV recommended for GCM
const TAG_LENGTH = 16; // 128-bit auth tag

// ── Helpers ───────────────────────────────────────────────────────────────────

function validateKey(key) {
  if (!Buffer.isBuffer(key) && !(key instanceof Uint8Array))
    throw new TypeError("key must be a Buffer or Uint8Array.");
  if (key.length !== 32)
    throw new RangeError("key must be exactly 32 bytes (AES-256).");
}

function validateData(data) {
  if (!Buffer.isBuffer(data) && !(data instanceof Uint8Array))
    throw new TypeError("data must be a Buffer or Uint8Array.");
}

// ── Core ──────────────────────────────────────────────────────────────────────

/**
 * Encrypt a Buffer with AES-256-GCM.
 *
 * @param {Buffer} data  - plaintext bytes
 * @param {Buffer} key   - 32-byte key
 * @returns {Buffer}     - IV + auth-tag + ciphertext
 */
function encryptBatchSwaps(data, key) {
  validateData(data);
  validateKey(key);

  const iv     = crypto.randomBytes(IV_LENGTH);
  const cipher = crypto.createCipheriv(ALGORITHM, key, iv, { authTagLength: TAG_LENGTH });
  const ct     = Buffer.concat([cipher.update(data), cipher.final()]);
  const tag    = cipher.getAuthTag();

  return Buffer.concat([iv, tag, ct]);
}

/**
 * Decrypt a Buffer produced by encryptBatchSwaps.
 * Throws if the key is wrong or the ciphertext has been tampered with.
 *
 * @param {Buffer} encrypted - IV + auth-tag + ciphertext
 * @param {Buffer} key       - 32-byte key
 * @returns {Buffer}         - plaintext bytes
 */
function decryptBatchSwaps(encrypted, key) {
  validateData(encrypted);
  validateKey(key);

  if (encrypted.length < IV_LENGTH + TAG_LENGTH)
    throw new RangeError("encrypted data is too short.");

  const iv      = encrypted.slice(0, IV_LENGTH);
  const tag     = encrypted.slice(IV_LENGTH, IV_LENGTH + TAG_LENGTH);
  const ct      = encrypted.slice(IV_LENGTH + TAG_LENGTH);
  const decipher = crypto.createDecipheriv(ALGORITHM, key, iv, { authTagLength: TAG_LENGTH });
  decipher.setAuthTag(tag);

  try {
    return Buffer.concat([decipher.update(ct), decipher.final()]);
  } catch {
    throw new Error("Decryption failed: invalid key or tampered data.");
  }
}

module.exports = { encryptBatchSwaps, decryptBatchSwaps };
