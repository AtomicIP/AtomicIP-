const crypto = require("crypto");
const { encryptBatchSwaps, decryptBatchSwaps } = require("../batch/batchEncryptor");

const key32  = () => crypto.randomBytes(32);
const plain  = () => Buffer.from(JSON.stringify([{ swapId: "s1", amount: 100 }]));

describe("encryptBatchSwaps — validation", () => {
  test("throws TypeError when data is not a Buffer", () => {
    expect(() => encryptBatchSwaps("not-a-buffer", key32())).toThrow(TypeError);
  });
  test("throws TypeError when key is not a Buffer", () => {
    expect(() => encryptBatchSwaps(plain(), "not-a-key")).toThrow(TypeError);
  });
  test("throws RangeError when key is wrong length", () => {
    expect(() => encryptBatchSwaps(plain(), Buffer.alloc(16))).toThrow(RangeError);
  });
});

describe("decryptBatchSwaps — validation", () => {
  test("throws TypeError when encrypted is not a Buffer", () => {
    expect(() => decryptBatchSwaps("not-a-buffer", key32())).toThrow(TypeError);
  });
  test("throws TypeError when key is not a Buffer", () => {
    expect(() => decryptBatchSwaps(Buffer.alloc(40), "not-a-key")).toThrow(TypeError);
  });
  test("throws RangeError when encrypted data is too short", () => {
    expect(() => decryptBatchSwaps(Buffer.alloc(10), key32())).toThrow(RangeError);
  });
});

describe("encryptBatchSwaps / decryptBatchSwaps — round-trip", () => {
  test("encrypts and decrypts back to original plaintext", () => {
    const key  = key32();
    const data = plain();
    expect(decryptBatchSwaps(encryptBatchSwaps(data, key), key)).toEqual(data);
  });

  test("empty Buffer round-trips correctly", () => {
    const key  = key32();
    const data = Buffer.alloc(0);
    expect(decryptBatchSwaps(encryptBatchSwaps(data, key), key)).toEqual(data);
  });

  test("large payload round-trips correctly", () => {
    const key  = key32();
    const data = Buffer.from(JSON.stringify(
      Array.from({ length: 100 }, (_, i) => ({ swapId: `s${i}`, amount: i * 10 }))
    ));
    expect(decryptBatchSwaps(encryptBatchSwaps(data, key), key)).toEqual(data);
  });

  test("each encryption produces a different ciphertext (random IV)", () => {
    const key  = key32();
    const data = plain();
    const c1   = encryptBatchSwaps(data, key);
    const c2   = encryptBatchSwaps(data, key);
    expect(c1.equals(c2)).toBe(false);
  });
});

describe("decryptBatchSwaps — wrong key / tampered data", () => {
  test("throws when decrypting with a different key", () => {
    const data      = plain();
    const encrypted = encryptBatchSwaps(data, key32());
    expect(() => decryptBatchSwaps(encrypted, key32())).toThrow();
  });

  test("throws when ciphertext bytes are tampered", () => {
    const key       = key32();
    const encrypted = encryptBatchSwaps(plain(), key);
    // Flip a byte in the ciphertext portion (after IV + tag)
    const tampered  = Buffer.from(encrypted);
    tampered[tampered.length - 1] ^= 0xff;
    expect(() => decryptBatchSwaps(tampered, key)).toThrow();
  });

  test("throws when auth-tag bytes are tampered", () => {
    const key       = key32();
    const encrypted = encryptBatchSwaps(plain(), key);
    const tampered  = Buffer.from(encrypted);
    tampered[12] ^= 0xff; // flip a byte in the 16-byte auth tag
    expect(() => decryptBatchSwaps(tampered, key)).toThrow();
  });
});
