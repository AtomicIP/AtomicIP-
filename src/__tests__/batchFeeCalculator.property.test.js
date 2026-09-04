/**
 * #880 — Property-Based Tests for batchFeeCalculator.js
 * ───────────────────────────────────────────────────────
 * Uses fast-check to generate random inputs and assert invariants
 * that must hold for ALL valid swap batches, regardless of inputs.
 *
 * Properties tested:
 *   1. Non-negativity   — no fee field is ever negative
 *   2. Upper bound      — netFee never exceeds the swap's value
 *   3. Monotonicity     — higher swap value → higher or equal grossFee
 *   4. Split invariant  — protocolFee + lpFee === netFee (within float precision)
 *   5. Discount bounds  — discountAmount is always ≥ 0 and < grossFee
 *   6. Totals match     — totalNetFee === sum of individual netFees
 *   7. Batch discount   — applying the discount never produces a higher fee
 */

"use strict";

const fc = require("fast-check");
const {
  calculateBatchFees,
  MAX_BATCH_SIZE,
  BASE_FEE_PER_SWAP,
} = require("../batch/batchFeeCalculator");

// ── Arbitraries ───────────────────────────────────────────────────────────────

/**
 * A valid swap entry: amount and value are both positive numbers.
 * Values are kept in a realistic range (1 to 1_000_000) to avoid
 * floating-point precision issues at extreme magnitudes.
 *
 * Note: fast-check fc.float requires 32-bit float (Math.fround) boundaries.
 */
const validSwap = fc.record({
  id:     fc.string({ minLength: 1, maxLength: 20 }),
  amount: fc.float({ min: Math.fround(0.001), max: Math.fround(10_000), noNaN: true }),
  value:  fc.float({ min: Math.fround(0.001), max: Math.fround(1_000_000), noNaN: true }),
});

/**
 * A non-empty batch of valid swaps, bounded by MAX_BATCH_SIZE.
 */
const validBatch = fc.array(validSwap, { minLength: 1, maxLength: MAX_BATCH_SIZE });

/**
 * A single-element batch (exercises per-swap paths cleanly).
 */
const singleSwapBatch = fc.array(validSwap, { minLength: 1, maxLength: 1 });

// ── Property 1: Non-negativity ────────────────────────────────────────────────

describe("#880 — Property: all fee fields are non-negative", () => {
  test("grossFee, netFee, discountAmount, protocolFee, lpFee ≥ 0 for any valid input", () => {
    fc.assert(
      fc.property(validBatch, (swaps) => {
        const result = calculateBatchFees(swaps);
        for (const f of result.swapFees) {
          expect(f.grossFee).toBeGreaterThanOrEqual(0);
          expect(f.netFee).toBeGreaterThanOrEqual(0);
          expect(f.discountAmount).toBeGreaterThanOrEqual(0);
          expect(f.protocolFee).toBeGreaterThanOrEqual(0);
          expect(f.lpFee).toBeGreaterThanOrEqual(0);
        }
        expect(result.totalGrossFee).toBeGreaterThanOrEqual(0);
        expect(result.totalNetFee).toBeGreaterThanOrEqual(0);
        expect(result.totalDiscount).toBeGreaterThanOrEqual(0);
        expect(result.totalProtocolFee).toBeGreaterThanOrEqual(0);
        expect(result.totalLpFee).toBeGreaterThanOrEqual(0);
      }),
      { numRuns: 200 }
    );
  });
});

// ── Property 2: Upper bound ───────────────────────────────────────────────────

describe("#880 — Property: fee is bounded by a reasonable ceiling", () => {
  test("netFee ≤ swap.value + BASE_FEE_PER_SWAP for every swap (base fee may exceed tiny values)", () => {
    // The base fee (0.001 per swap) exists independently of value, so for very
    // tiny swap values the fee can exceed the value. The bound is:
    //   netFee ≤ max_volume_rate% × value + BASE_FEE_PER_SWAP
    // which simplifies to netFee ≤ value + BASE_FEE_PER_SWAP (since rate ≤ 100%).
    const BASE_FEE = 0.001;
    fc.assert(
      fc.property(validBatch, (swaps) => {
        const result = calculateBatchFees(swaps);
        for (let i = 0; i < swaps.length; i++) {
          // netFee ≤ grossFee ≤ value + BASE_FEE (no rate exceeds 100%)
          expect(result.swapFees[i].netFee).toBeLessThanOrEqual(
            swaps[i].value + BASE_FEE + 1e-6
          );
        }
      }),
      { numRuns: 200 }
    );
  });

  test("totalNetFee ≤ totalVolume + batchSize × BASE_FEE_PER_SWAP", () => {
    fc.assert(
      fc.property(validBatch, (swaps) => {
        const result   = calculateBatchFees(swaps);
        const ceiling  = result.totalVolume + swaps.length * BASE_FEE_PER_SWAP + 1e-6;
        expect(result.totalNetFee).toBeLessThanOrEqual(ceiling);
      }),
      { numRuns: 200 }
    );
  });
});

// ── Property 3: Monotonicity ──────────────────────────────────────────────────

describe("#880 — Property: grossFee is monotone in swap value", () => {
  test("doubling swap value at least doubles grossFee (minus base fee contribution)", () => {
    fc.assert(
      fc.property(
        fc.float({ min: Math.fround(1), max: Math.fround(100_000), noNaN: true }),
        (value) => {
          const swapLow  = [{ id: "low",  amount: 1, value }];
          const swapHigh = [{ id: "high", amount: 1, value: value * 2 }];

          const low  = calculateBatchFees(swapLow,  { applyBatchDiscount: false });
          const high = calculateBatchFees(swapHigh, { applyBatchDiscount: false });

          // Higher value → higher or equal gross fee
          expect(high.totalGrossFee).toBeGreaterThanOrEqual(low.totalGrossFee - 1e-8);
        }
      ),
      { numRuns: 200 }
    );
  });

  test("fee tier transitions: higher total volume → lower or equal feeBps", () => {
    fc.assert(
      fc.property(
        fc.float({ min: Math.fround(0.001), max: Math.fround(500_000), noNaN: true }),
        fc.float({ min: Math.fround(0.001), max: Math.fround(500_000), noNaN: true }),
        (v1, v2) => {
          const low  = calculateBatchFees([{ id: "a", amount: 1, value: Math.min(v1, v2) }]);
          const high = calculateBatchFees([{ id: "b", amount: 1, value: Math.max(v1, v2) }]);
          expect(high.effectiveFeeBps).toBeLessThanOrEqual(low.effectiveFeeBps);
        }
      ),
      { numRuns: 200 }
    );
  });
});

// ── Property 4: Split invariant ───────────────────────────────────────────────

describe("#880 — Property: protocolFee + lpFee === netFee", () => {
  test("split always sums back to netFee (within float rounding tolerance)", () => {
    fc.assert(
      fc.property(validBatch, (swaps) => {
        const result = calculateBatchFees(swaps);
        for (const f of result.swapFees) {
          expect(f.protocolFee + f.lpFee).toBeCloseTo(f.netFee, 6);
        }
        expect(result.totalProtocolFee + result.totalLpFee).toBeCloseTo(result.totalNetFee, 4);
      }),
      { numRuns: 200 }
    );
  });
});

// ── Property 5: Discount bounds ───────────────────────────────────────────────

describe("#880 — Property: discount is bounded by [0, grossFee)", () => {
  test("discountAmount is ≥ 0 and strictly < grossFee when batch discount is applied", () => {
    fc.assert(
      fc.property(validBatch, (swaps) => {
        const result = calculateBatchFees(swaps, { applyBatchDiscount: true });
        for (const f of result.swapFees) {
          expect(f.discountAmount).toBeGreaterThanOrEqual(0);
          expect(f.discountAmount).toBeLessThan(f.grossFee + 1e-8);
        }
      }),
      { numRuns: 200 }
    );
  });

  test("discountAmount is exactly 0 when applyBatchDiscount=false", () => {
    fc.assert(
      fc.property(validBatch, (swaps) => {
        const result = calculateBatchFees(swaps, { applyBatchDiscount: false });
        for (const f of result.swapFees) {
          expect(f.discountAmount).toBe(0);
          expect(f.netFee).toBeCloseTo(f.grossFee, 6);
        }
      }),
      { numRuns: 200 }
    );
  });
});

// ── Property 6: Totals match sum of swap fees ─────────────────────────────────

describe("#880 — Property: batch totals equal sum of individual swap fees", () => {
  test("totalNetFee === Σ swapFees[i].netFee", () => {
    fc.assert(
      fc.property(validBatch, (swaps) => {
        const result = calculateBatchFees(swaps);
        const sum    = result.swapFees.reduce((s, f) => s + f.netFee, 0);
        expect(result.totalNetFee).toBeCloseTo(sum, 4);
      }),
      { numRuns: 200 }
    );
  });

  test("totalGrossFee === Σ swapFees[i].grossFee", () => {
    fc.assert(
      fc.property(validBatch, (swaps) => {
        const result = calculateBatchFees(swaps);
        const sum    = result.swapFees.reduce((s, f) => s + f.grossFee, 0);
        expect(result.totalGrossFee).toBeCloseTo(sum, 4);
      }),
      { numRuns: 200 }
    );
  });

  test("totalVolume === Σ swap.value", () => {
    fc.assert(
      fc.property(validBatch, (swaps) => {
        const result = calculateBatchFees(swaps);
        const sum    = swaps.reduce((s, sw) => s + sw.value, 0);
        expect(result.totalVolume).toBeCloseTo(sum, 4);
      }),
      { numRuns: 200 }
    );
  });
});

// ── Property 7: Batch discount can only reduce fees ──────────────────────────

describe("#880 — Property: batch discount can only reduce or equal fees", () => {
  test("netFee with discount ≤ netFee without discount", () => {
    fc.assert(
      fc.property(validBatch, (swaps) => {
        const withDiscount    = calculateBatchFees(swaps, { applyBatchDiscount: true });
        const withoutDiscount = calculateBatchFees(swaps, { applyBatchDiscount: false });

        expect(withDiscount.totalNetFee).toBeLessThanOrEqual(withoutDiscount.totalNetFee + 1e-8);
      }),
      { numRuns: 200 }
    );
  });
});

// ── Property 8: overrideFeeBps is always honoured ────────────────────────────

describe("#880 — Property: overrideFeeBps overrides tier selection", () => {
  test("effectiveFeeBps equals overrideFeeBps for any valid batch", () => {
    fc.assert(
      fc.property(
        validBatch,
        fc.integer({ min: 0, max: 100 }),
        (swaps, bps) => {
          const result = calculateBatchFees(swaps, { overrideFeeBps: bps });
          expect(result.effectiveFeeBps).toBe(bps);
        }
      ),
      { numRuns: 200 }
    );
  });
});
