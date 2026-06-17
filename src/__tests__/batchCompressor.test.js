const {
  compressBatchSwaps,
  decompressBatchSwaps,
  MAX_BATCH_SIZE,
} = require("../batch/batchCompressor");

const swap = (id) => ({ swapId: id, state: "PENDING", amount: 1000 });

describe("compressBatchSwaps — validation", () => {
  test("throws TypeError on empty array", () => {
    expect(() => compressBatchSwaps([])).toThrow(TypeError);
  });
  test("throws TypeError on non-array", () => {
    expect(() => compressBatchSwaps(null)).toThrow(TypeError);
  });
  test("throws RangeError when batch exceeds MAX_BATCH_SIZE", () => {
    const big = Array.from({ length: MAX_BATCH_SIZE + 1 }, (_, i) => swap(`s${i}`));
    expect(() => compressBatchSwaps(big)).toThrow(RangeError);
  });
  test("throws TypeError when a swap entry is not an object", () => {
    expect(() => compressBatchSwaps(["not-an-object"])).toThrow(TypeError);
  });
});

describe("decompressBatchSwaps — validation", () => {
  test("throws TypeError on non-Buffer input", () => {
    expect(() => decompressBatchSwaps("not-a-buffer")).toThrow(TypeError);
  });
  test("throws on corrupted bytes", () => {
    expect(() => decompressBatchSwaps(Buffer.from("garbage"))).toThrow();
  });
});

describe("compressBatchSwaps / decompressBatchSwaps — round-trip", () => {
  test("single swap round-trips correctly", () => {
    const swaps = [swap("a1")];
    expect(decompressBatchSwaps(compressBatchSwaps(swaps))).toEqual(swaps);
  });

  test("multiple swaps round-trip correctly", () => {
    const swaps = [swap("b1"), swap("b2"), swap("b3")];
    expect(decompressBatchSwaps(compressBatchSwaps(swaps))).toEqual(swaps);
  });

  test("large batch (MAX_BATCH_SIZE) round-trips correctly", () => {
    const swaps = Array.from({ length: MAX_BATCH_SIZE }, (_, i) => swap(`l${i}`));
    expect(decompressBatchSwaps(compressBatchSwaps(swaps))).toEqual(swaps);
  });

  test("compressed output is smaller than raw JSON for large batch", () => {
    const swaps = Array.from({ length: 50 }, (_, i) => ({
      swapId: `swap-${i}`,
      state: "PENDING",
      amount: 1000 + i,
      seller: "GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
      buyer:  "GYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYY",
    }));
    const compressed = compressBatchSwaps(swaps);
    const raw = Buffer.from(JSON.stringify(swaps), "utf8");
    expect(compressed.length).toBeLessThan(raw.length);
  });

  test("swap with nested fields round-trips correctly", () => {
    const swaps = [{ swapId: "n1", state: "ACTIVE", amount: 500, meta: { tag: "test" } }];
    expect(decompressBatchSwaps(compressBatchSwaps(swaps))).toEqual(swaps);
  });
});
