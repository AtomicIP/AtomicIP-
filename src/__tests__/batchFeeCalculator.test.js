const {
  calculateBatchFees,
  getTierRate,
  FEE_TIERS,
  BASE_FEE_PER_SWAP,
  MAX_BATCH_SIZE,
} = require("../batch/batchFeeCalculator");

describe("getTierRate", () => {
  test("returns default 30bps for low volume", () => {
    expect(getTierRate(0)).toBe(30);
    expect(getTierRate(9999)).toBe(30);
  });
  test("returns 25bps at 10k threshold", () => {
    expect(getTierRate(10_000)).toBe(25);
  });
  test("returns 15bps at institutional threshold", () => {
    expect(getTierRate(1_000_000)).toBe(15);
  });
});

describe("calculateBatchFees — validation", () => {
  test("throws on empty array", () => {
    expect(() => calculateBatchFees([])).toThrow(TypeError);
  });
  test("throws on non-array", () => {
    expect(() => calculateBatchFees(null)).toThrow(TypeError);
  });
  test("throws on batch size > MAX_BATCH_SIZE", () => {
    const big = Array.from({ length: MAX_BATCH_SIZE + 1 }, (_, i) => ({ amount: 1, value: 100 }));
    expect(() => calculateBatchFees(big)).toThrow(RangeError);
  });
  test("throws on negative amount", () => {
    expect(() => calculateBatchFees([{ amount: -1, value: 100 }])).toThrow(RangeError);
  });
  test("throws on zero value", () => {
    expect(() => calculateBatchFees([{ amount: 1, value: 0 }])).toThrow(RangeError);
  });
});

describe("calculateBatchFees — fee maths", () => {
  const swaps = [
    { id: "a", amount: 10, value: 1000 },
    { id: "b", amount: 5,  value: 500  },
  ];

  test("returns correct batch size and total volume", () => {
    const result = calculateBatchFees(swaps);
    expect(result.batchSize).toBe(2);
    expect(result.totalVolume).toBeCloseTo(1500);
  });

  test("netFee < grossFee when batch discount applied", () => {
    const result = calculateBatchFees(swaps);
    result.swapFees.forEach((f) => {
      expect(f.netFee).toBeLessThan(f.grossFee);
    });
  });

  test("protocolFee + lpFee === netFee for each swap", () => {
    const result = calculateBatchFees(swaps);
    result.swapFees.forEach((f) => {
      expect(f.protocolFee + f.lpFee).toBeCloseTo(f.netFee, 6);
    });
  });

  test("totals match sum of individual swap fees", () => {
    const result = calculateBatchFees(swaps);
    const sumNet = result.swapFees.reduce((s, f) => s + f.netFee, 0);
    expect(result.totalNetFee).toBeCloseTo(sumNet, 6);
  });

  test("overrideFeeBps is respected", () => {
    const result = calculateBatchFees(swaps, { overrideFeeBps: 10 });
    expect(result.effectiveFeeBps).toBe(10);
  });

  test("no discount when applyBatchDiscount=false", () => {
    const result = calculateBatchFees(swaps, { applyBatchDiscount: false });
    result.swapFees.forEach((f) => expect(f.discountAmount).toBe(0));
  });

  test("uses lower tier for high-volume batch", () => {
    const bigSwaps = [{ amount: 1, value: 1_000_000 }];
    const result = calculateBatchFees(bigSwaps);
    expect(result.effectiveFeeBps).toBe(15);
  });
});
