const {
  calculateBatchPrices,
  applyBatchPriceAdjustment,
  validateBatchPriceBounds,
  resolveVolumeTierDiscount,
  applyDiscount,
  DEFAULT_VOLUME_TIERS,
  MAX_BATCH_SIZE,
  BPS_DENOM,
} = require("../batch/batchPricing");

const swapsOfSize = (size, basePrice = 10_000) =>
  Array.from({ length: size }, (_, index) => ({
    swapId: `swap-${index}`,
    basePrice,
  }));

describe("resolveVolumeTierDiscount — tier boundaries", () => {
  test.each([
    [0, 0],
    [1, 0],
    [4, 0],
    [5, 50],
    [9, 50],
    [10, 100],
    [24, 100],
    [25, 200],
    [49, 200],
    [50, 300],
    [MAX_BATCH_SIZE, 300],
  ])("count %i resolves to %ibps", (count, expected) => {
    expect(resolveVolumeTierDiscount(count)).toBe(expected);
  });

  test("uses the highest matching custom tier", () => {
    const tiers = [
      { minCount: 1, discountBps: 10 },
      { minCount: 3, discountBps: 25 },
      { minCount: 8, discountBps: 75 },
    ];

    expect(resolveVolumeTierDiscount(7, tiers)).toBe(25);
    expect(resolveVolumeTierDiscount(8, tiers)).toBe(75);
  });
});

describe("calculateBatchPrices — tiered pricing", () => {
  test("applies no discount below the first discount tier", () => {
    const result = calculateBatchPrices(swapsOfSize(4));

    expect(result.volumeDiscountBps).toBe(0);
    expect(result.prices.every((price) => price.priceSource === "base")).toBe(true);
    expect(result.totalFinalValue).toBe(result.totalBaseValue);
  });

  test("applies each discount at its inclusive boundary", () => {
    const expected = [
      [5, 50, 9_950],
      [10, 100, 9_900],
      [25, 200, 9_800],
      [50, 300, 9_700],
    ];

    expected.forEach(([count, discountBps, finalPrice]) => {
      const result = calculateBatchPrices(swapsOfSize(count));
      expect(result.volumeDiscountBps).toBe(discountBps);
      expect(result.prices[0].finalPrice).toBe(finalPrice);
      expect(result.prices[0].discountAmount).toBe(10_000 - finalPrice);
      expect(result.prices.every((price) => price.priceSource === "volume_tier")).toBe(true);
    });
  });

  test("keeps overrides and valid oracle prices out of volume discounts", () => {
    const result = calculateBatchPrices(
      [
        { swapId: "override", basePrice: 100, overridePrice: 80 },
        { swapId: "oracle", basePrice: 100 },
        { swapId: "fallback", basePrice: 100 },
      ],
      { oracleFn: (swapId) => (swapId === "oracle" ? 70 : 0) }
    );

    expect(result.prices.map(({ finalPrice, priceSource, discountBps }) => [finalPrice, priceSource, discountBps]))
      .toEqual([[80, "override", 0], [70, "oracle", 0], [100, "base", 0]]);
  });
});

describe("batch pricing — rounding and monotonicity", () => {
  test("floors discount amounts before subtracting them", () => {
    expect(applyDiscount(101, 50)).toBe(101);
    expect(applyDiscount(10_001, 50)).toBe(9_951);
    expect(calculateBatchPrices(swapsOfSize(5, 101)).prices[0].finalPrice).toBe(101);
  });

  test("price is non-increasing as batch quantity grows", () => {
    const prices = Array.from({ length: MAX_BATCH_SIZE }, (_, index) =>
      calculateBatchPrices(swapsOfSize(index + 1, 10_001)).prices[0].finalPrice
    );

    prices.slice(1).forEach((price, index) => {
      expect(price).toBeLessThanOrEqual(prices[index]);
    });
  });

  test("uses the exported basis-point denominator", () => {
    expect(applyDiscount(20_000, BPS_DENOM)).toBe(0);
  });
});

describe("calculateBatchPrices — validation and bounds", () => {
  test("rejects invalid batches and entries", () => {
    expect(() => calculateBatchPrices([])).toThrow(TypeError);
    expect(() => calculateBatchPrices(swapsOfSize(MAX_BATCH_SIZE + 1))).toThrow(RangeError);
    expect(() => calculateBatchPrices([{ swapId: "bad", basePrice: 0 }])).toThrow(RangeError);
  });

  test("enforces price floors and ceilings", () => {
    expect(() => calculateBatchPrices([{ swapId: "floor", basePrice: 100 }], { priceFloor: 101 }))
      .toThrow(/below floor/);
    expect(() => calculateBatchPrices([{ swapId: "ceiling", basePrice: 100 }], { priceCeiling: 99 }))
      .toThrow(/exceeds ceiling/);
  });
});

describe("applyBatchPriceAdjustment", () => {
  test("rounds markup and markdown deltas down", () => {
    const swaps = [{ swapId: "a", basePrice: 101 }];

    expect(applyBatchPriceAdjustment(swaps, 50)[0].adjustedPrice).toBe(101);
    expect(applyBatchPriceAdjustment(swaps, -50)[0].adjustedPrice).toBe(101);
  });

  test("does not reduce a markdown below one", () => {
    expect(applyBatchPriceAdjustment([{ swapId: "a", basePrice: 1 }], -BPS_DENOM)[0].adjustedPrice).toBe(1);
  });
});

describe("validateBatchPriceBounds", () => {
  test("separates valid entries and reports each violation", () => {
    const result = validateBatchPriceBounds(
      [{ swapId: "ok", price: 50 }, { swapId: "bad", price: 150 }],
      { floor: 10, ceiling: 100 }
    );

    expect(result.valid).toEqual([{ swapId: "ok", price: 50 }]);
    expect(result.invalid).toEqual([{
      swapId: "bad",
      price: 150,
      violations: ["above ceiling 100"],
    }]);
  });
});

test("default tiers are ordered by increasing minimum count", () => {
  expect(DEFAULT_VOLUME_TIERS.map((tier) => tier.minCount)).toEqual([1, 5, 10, 25, 50]);
});