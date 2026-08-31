const {
  calculateSwapRoyalty,
  validateRoyaltyConfig,
  MAX_ROYALTY_RATE_BPS,
  BPS_DENOM,
} = require("../batch/batchRoyaltyDistributor");

// ── Fixtures ──────────────────────────────────────────────────────────────

const sampleRoyaltyConfig = {
  assetId: "asset-123",
  rateBps: 500, // 5%
  beneficiaries: [
    { id: "ben-1", shareBps: 5000 }, // 50%
    { id: "ben-2", shareBps: 5000 }, // 50%
  ],
};

// ── #916: calculateSwapRoyalty ────────────────────────────────────────────

describe("calculateSwapRoyalty — basic calculation", () => {
  test("calculates royalty from sale price and rate", () => {
    const result = calculateSwapRoyalty("s1", 1000, sampleRoyaltyConfig);
    expect(result.swapId).toBe("s1");
    expect(result.totalRoyalty).toBe(50); // 1000 * 5% = 50
    expect(result.sellerProceeds).toBe(950); // 1000 - 50
  });

  test("distributes royalty across beneficiaries proportionally", () => {
    const result = calculateSwapRoyalty("s2", 1000, sampleRoyaltyConfig);
    expect(result.payouts).toHaveLength(2);
    expect(result.payouts[0].amount).toBe(25); // 50 * 50% = 25
    expect(result.payouts[1].amount).toBe(25); // 50 * 50% = 25
  });

  test("includes asset info in result", () => {
    const result = calculateSwapRoyalty("s3", 2000, sampleRoyaltyConfig);
    expect(result.assetId).toBe("asset-123");
    expect(result.rateBps).toBe(500);
    expect(result.salePrice).toBe(2000);
  });

  test("handles zero sale price gracefully", () => {
    const result = calculateSwapRoyalty("s4", 0, sampleRoyaltyConfig);
    expect(result.totalRoyalty).toBe(0);
    expect(result.sellerProceeds).toBe(0);
  });

  test("handles maximum royalty rate", () => {
    const maxConfig = {
      assetId: "asset-max",
      rateBps: MAX_ROYALTY_RATE_BPS, // 30%
      beneficiaries: [{ id: "ben-max", shareBps: BPS_DENOM }],
    };
    const result = calculateSwapRoyalty("s-max", 1000, maxConfig);
    expect(result.totalRoyalty).toBe(300); // 1000 * 30% = 300
  });

  test("assigns rounding dust to first beneficiary", () => {
    // Price that will cause rounding dust
    const config = {
      assetId: "asset-dust",
      rateBps: 333, // 3.33%
      beneficiaries: [
        { id: "b1", shareBps: 3333 },
        { id: "b2", shareBps: 3333 },
        { id: "b3", shareBps: 3334 },
      ],
    };
    const result = calculateSwapRoyalty("s-dust", 1000, config);
    const totalDistributed = result.payouts.reduce((s, p) => s + p.amount, 0);
    expect(totalDistributed).toBe(result.totalRoyalty);
    // First beneficiary should have the dust
    expect(result.payouts[0].amount).toBeGreaterThanOrEqual(
      result.payouts[1].amount
    );
  });
});

describe("calculateSwapRoyalty — single beneficiary", () => {
  test("single beneficiary gets entire royalty", () => {
    const config = {
      assetId: "single-asset",
      rateBps: 1000, // 10%
      beneficiaries: [{ id: "sole-ben", shareBps: BPS_DENOM }],
    };
    const result = calculateSwapRoyalty("s-single", 1000, config);
    expect(result.totalRoyalty).toBe(100);
    expect(result.payouts).toHaveLength(1);
    expect(result.payouts[0].amount).toBe(100);
    expect(result.payouts[0].beneficiaryId).toBe("sole-ben");
  });
});

describe("calculateSwapRoyalty — multiple beneficiaries", () => {
  test("three-way split", () => {
    const config = {
      assetId: "three-way",
      rateBps: 1000, // 10%
      beneficiaries: [
        { id: "b1", shareBps: 3333 },
        { id: "b2", shareBps: 3333 },
        { id: "b3", shareBps: 3334 },
      ],
    };
    const result = calculateSwapRoyalty("s-three", 1000, config);
    expect(result.totalRoyalty).toBe(100);
    const total = result.payouts.reduce((s, p) => s + p.amount, 0);
    expect(total).toBe(100);
  });

  test("unequal beneficiary shares", () => {
    const config = {
      assetId: "unequal",
      rateBps: 1000, // 10%
      beneficiaries: [
        { id: "creator", shareBps: 7000 }, // 70%
        { id: "platform", shareBps: 3000 }, // 30%
      ],
    };
    const result = calculateSwapRoyalty("s-unequal", 1000, config);
    expect(result.totalRoyalty).toBe(100);
    expect(result.payouts[0].amount).toBe(70); // creator
    expect(result.payouts[1].amount).toBe(30); // platform
  });
});
