/**
 * #882 — batchCompressor Round-Trip Fuzz Test
 * ─────────────────────────────────────────────
 * Uses fast-check to assert that compress → decompress is a lossless
 * identity for any valid swap batch, regardless of the structure,
 * values, or nesting depth of the swap objects.
 *
 * Also asserts the compression-ratio property: for representative
 * payloads large enough to compress well, the compressed output
 * must be smaller than the raw JSON.
 */

"use strict";

const fc = require("fast-check");
const { compressBatchSwaps, decompressBatchSwaps, MAX_BATCH_SIZE } = require("../batch/batchCompressor");

// ── Arbitraries ───────────────────────────────────────────────────────────────

/**
 * Generate a single swap object with arbitrary structure.
 * The swap is guaranteed to be a non-null object (required by the module).
 *
 * Note: fast-check fc.float requires 32-bit float (Math.fround) boundaries.
 */
const swapArb = fc.record({
  swapId:    fc.oneof(fc.string(), fc.integer()),
  state:     fc.constantFrom("PENDING", "ACTIVE", "COMPLETED", "CANCELLED"),
  amount:    fc.float({ min: Math.fround(0.001), max: Math.fround(1_000_000), noNaN: true }),
  salePrice: fc.option(fc.float({ min: Math.fround(1), max: Math.fround(999_999), noNaN: true })),
  seller:    fc.string({ minLength: 1, maxLength: 60 }),
  buyer:     fc.string({ minLength: 1, maxLength: 60 }),
  ipId:      fc.option(fc.nat()),
  meta:      fc.option(fc.record({ tag: fc.string(), version: fc.nat() })),
});

/** A swap with deeply nested metadata, to stress JSON serialisation. */
const deepSwapArb = fc.record({
  swapId:  fc.string(),
  state:   fc.string(),
  amount:  fc.float({ min: Math.fround(0.001), max: Math.fround(10_000), noNaN: true }),
  meta: fc.record({
    level1: fc.record({
      level2: fc.record({
        level3: fc.string(),
        value:  fc.nat(),
      }),
    }),
  }),
});

/** A valid batch: 1–MAX_BATCH_SIZE swap objects. */
const batchArb = fc.array(swapArb, { minLength: 1, maxLength: MAX_BATCH_SIZE });

// ── Property: round-trip correctness ──────────────────────────────────────────

describe("#882 — Property: compress → decompress is a lossless identity", () => {
  test("any valid batch round-trips back to the original array", () => {
    fc.assert(
      fc.property(batchArb, (swaps) => {
        const compressed   = compressBatchSwaps(swaps);
        const decompressed = decompressBatchSwaps(compressed);
        expect(JSON.stringify(decompressed)).toBe(JSON.stringify(swaps));
      }),
      { numRuns: 300, verbose: false }
    );
  });

  test("single-element batch always round-trips correctly", () => {
    fc.assert(
      fc.property(swapArb, (swap) => {
        const swaps        = [swap];
        const compressed   = compressBatchSwaps(swaps);
        const decompressed = decompressBatchSwaps(compressed);
        expect(decompressed).toEqual(swaps);
      }),
      { numRuns: 300, verbose: false }
    );
  });

  test("deeply nested swap objects round-trip without data loss", () => {
    fc.assert(
      fc.property(
        fc.array(deepSwapArb, { minLength: 1, maxLength: 20 }),
        (swaps) => {
          const decompressed = decompressBatchSwaps(compressBatchSwaps(swaps));
          expect(decompressed).toEqual(swaps);
        }
      ),
      { numRuns: 200 }
    );
  });

  test("maximum-size batch (100 swaps) always round-trips correctly", () => {
    fc.assert(
      fc.property(
        fc.array(swapArb, { minLength: MAX_BATCH_SIZE, maxLength: MAX_BATCH_SIZE }),
        (swaps) => {
          const decompressed = decompressBatchSwaps(compressBatchSwaps(swaps));
          expect(JSON.stringify(decompressed)).toBe(JSON.stringify(swaps));
        }
      ),
      { numRuns: 50 }
    );
  });
});

// ── Property: compressed output is a Buffer ────────────────────────────────────

describe("#882 — Property: compressBatchSwaps always returns a Buffer", () => {
  test("output is always a Buffer instance", () => {
    fc.assert(
      fc.property(batchArb, (swaps) => {
        const compressed = compressBatchSwaps(swaps);
        expect(Buffer.isBuffer(compressed)).toBe(true);
      }),
      { numRuns: 200 }
    );
  });
});

// ── Property: compression actually reduces size ───────────────────────────────

describe("#882 — Property: compression ratio (size reduction) for larger batches", () => {
  test("compressed output is smaller than raw JSON for batches of ≥ 10 repetitive swaps", () => {
    fc.assert(
      fc.property(
        // Generate a single swap template and replicate it — maximises repetition
        swapArb.chain((tmpl) =>
          fc
            .integer({ min: 10, max: MAX_BATCH_SIZE })
            .map((n) => Array.from({ length: n }, (_, i) => ({ ...tmpl, swapId: `${tmpl.swapId}-${i}` })))
        ),
        (swaps) => {
          const raw        = Buffer.byteLength(JSON.stringify(swaps), "utf8");
          const compressed = compressBatchSwaps(swaps);
          // Repetitive payloads should always compress
          expect(compressed.length).toBeLessThan(raw);
        }
      ),
      { numRuns: 100 }
    );
  });

  test("compression ratio is ≥ 1.5× for the canonical large payload", () => {
    // Use a deterministic large payload representative of real batches
    const swaps = Array.from({ length: 100 }, (_, i) => ({
      swapId:    `swap-${i}`,
      state:     i % 2 === 0 ? "PENDING" : "COMPLETED",
      amount:    1000 + i,
      salePrice: 50_000 + i * 100,
      seller:    "GCSELLER" + "X".repeat(48),
      buyer:     "GCBUYER"  + "Y".repeat(49),
      timestamp: 1_700_000_000 + i,
    }));

    const raw        = Buffer.byteLength(JSON.stringify(swaps), "utf8");
    const compressed = compressBatchSwaps(swaps);
    const ratio      = raw / compressed.length;

    expect(ratio).toBeGreaterThanOrEqual(1.5);
  });
});

// ── Deterministic regression cases ────────────────────────────────────────────

describe("#882 — Deterministic round-trip regression cases", () => {
  const cases = [
    {
      label: "swap with null/undefined-equivalent optional fields (undefined serialises away)",
      swaps: [{ swapId: "rt-1", state: "PENDING", amount: 42, seller: "G1", buyer: "G2" }],
    },
    {
      label: "swap with special characters in strings",
      swaps: [{ swapId: "rt-2", state: "ACTIVE", amount: 1, seller: '{"nested": true}', buyer: "GCBUYER" }],
    },
    {
      label: "swap with numeric swapId",
      swaps: [{ swapId: 12345, state: "COMPLETED", amount: 500 }],
    },
    {
      label: "swap with Unicode in string fields",
      swaps: [{ swapId: "rt-unicode", state: "PENDING", amount: 1, seller: "日本語テスト", buyer: "عربى" }],
    },
    {
      label: "batch with all identical swaps (maximum repetition)",
      swaps: Array.from({ length: 100 }, () => ({ swapId: "dup", state: "PENDING", amount: 1000 })),
    },
  ];

  for (const { label, swaps } of cases) {
    test(label, () => {
      expect(decompressBatchSwaps(compressBatchSwaps(swaps))).toEqual(swaps);
    });
  }
});
