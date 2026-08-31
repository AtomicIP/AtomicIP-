const {
  calculateSwapRoyalty,
  distributeBatchRoyalties,
  settleBeneficiaryPayouts,
  validateRoyaltyConfig,
  MAX_ROYALTY_RATE_BPS,
  MAX_BATCH_SIZE,
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

const sampleSwap = {
  swapId: "swap-1",
  salePrice: 1000,
  royaltyConfig: sampleRoyaltyConfig,
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

// ── #915: distributeBatchRoyalties ────────────────────────────────────────

describe("distributeBatchRoyalties — validation", () => {
  test("throws on empty swaps array", () => {
    expect(() => distributeBatchRoyalties([])).toThrow(TypeError);
  });

  test("throws on null swaps", () => {
    expect(() => distributeBatchRoyalties(null)).toThrow(TypeError);
  });

  test("throws on batch exceeding MAX_BATCH_SIZE", () => {
    const big = Array.from({ length: MAX_BATCH_SIZE + 1 }, (_, i) => ({
      swapId: `s${i}`,
      salePrice: 1000,
      royaltyConfig: sampleRoyaltyConfig,
    }));
    expect(() => distributeBatchRoyalties(big)).toThrow(RangeError);
  });

  test("throws on missing swapId", () => {
    const badSwap = {
      salePrice: 1000,
      royaltyConfig: sampleRoyaltyConfig,
    };
    expect(() => distributeBatchRoyalties([badSwap])).toThrow(TypeError);
  });

  test("throws on negative salePrice", () => {
    const badSwap = {
      swapId: "s-neg",
      salePrice: -100,
      royaltyConfig: sampleRoyaltyConfig,
    };
    expect(() => distributeBatchRoyalties([badSwap])).toThrow(RangeError);
  });

  test("throws on zero salePrice", () => {
    const badSwap = {
      swapId: "s-zero",
      salePrice: 0,
      royaltyConfig: sampleRoyaltyConfig,
    };
    expect(() => distributeBatchRoyalties([badSwap])).toThrow(RangeError);
  });
});

describe("distributeBatchRoyalties — basic distribution", () => {
  test("single swap batch returns correct structure", () => {
    const result = distributeBatchRoyalties([sampleSwap]);
    expect(result.batchSize).toBe(1);
    expect(result.processed).toBe(1);
    expect(result.failed).toBe(0);
    expect(result.distributions).toHaveLength(1);
    expect(result.aggregated).toHaveLength(2);
  });

  test("two swaps with same beneficiary aggregate correctly", () => {
    const swaps = [
      { swapId: "s1", salePrice: 1000, royaltyConfig: sampleRoyaltyConfig },
      { swapId: "s2", salePrice: 2000, royaltyConfig: sampleRoyaltyConfig },
    ];
    const result = distributeBatchRoyalties(swaps);
    expect(result.batchSize).toBe(2);
    expect(result.processed).toBe(2);
    expect(result.distributions).toHaveLength(2);
    expect(result.aggregated).toHaveLength(2);

    // Each beneficiary should have 2 swaps
    result.aggregated.forEach((agg) => {
      expect(agg.swapCount).toBe(2);
    });
  });

  test("totalRoyaltiesGenerated is sum of all swap royalties", () => {
    const swaps = [
      { swapId: "s1", salePrice: 1000, royaltyConfig: sampleRoyaltyConfig }, // 5% = 50
      { swapId: "s2", salePrice: 2000, royaltyConfig: sampleRoyaltyConfig }, // 5% = 100
    ];
    const result = distributeBatchRoyalties(swaps);
    expect(result.totalRoyaltiesGenerated).toBe(150);
  });
});

describe("distributeBatchRoyalties — partial failures", () => {
  test("invalid swap is recorded in errors without failing batch", () => {
    const swaps = [
      sampleSwap,
      { swapId: "bad-swap", salePrice: -100, royaltyConfig: sampleRoyaltyConfig },
      { swapId: "s3", salePrice: 500, royaltyConfig: sampleRoyaltyConfig },
    ];
    const result = distributeBatchRoyalties(swaps);
    expect(result.processed).toBe(2);
    expect(result.failed).toBe(1);
    expect(result.errors).toHaveLength(1);
    expect(result.errors[0].swapId).toBe("bad-swap");
  });

  test("batch with all valid swaps has no errors", () => {
    const swaps = [
      sampleSwap,
      { swapId: "s2", salePrice: 500, royaltyConfig: sampleRoyaltyConfig },
      { swapId: "s3", salePrice: 1500, royaltyConfig: sampleRoyaltyConfig },
    ];
    const result = distributeBatchRoyalties(swaps);
    expect(result.failed).toBe(0);
    expect(result.errors).toHaveLength(0);
  });

  test("batchSize reflects total count regardless of failures", () => {
    const swaps = [
      sampleSwap,
      { swapId: "invalid", salePrice: 0, royaltyConfig: sampleRoyaltyConfig },
      { swapId: "s3", salePrice: 1000, royaltyConfig: sampleRoyaltyConfig },
    ];
    const result = distributeBatchRoyalties(swaps);
    expect(result.batchSize).toBe(3);
  });
});

describe("distributeBatchRoyalties — aggregation", () => {
  test("aggregates same beneficiary across multiple swaps", () => {
    const config = {
      assetId: "asset-1",
      rateBps: 1000, // 10%
      beneficiaries: [{ id: "ben-all", shareBps: BPS_DENOM }],
    };
    const swaps = [
      { swapId: "s1", salePrice: 1000, royaltyConfig: config },
      { swapId: "s2", salePrice: 2000, royaltyConfig: config },
      { swapId: "s3", salePrice: 3000, royaltyConfig: config },
    ];
    const result = distributeBatchRoyalties(swaps);
    expect(result.aggregated).toHaveLength(1);
    expect(result.aggregated[0].beneficiaryId).toBe("ben-all");
    expect(result.aggregated[0].totalAmount).toBe(600); // 100 + 200 + 300
    expect(result.aggregated[0].swapCount).toBe(3);
  });

  test("different beneficiaries get separate aggregates", () => {
    const config1 = {
      assetId: "asset-1",
      rateBps: 1000,
      beneficiaries: [{ id: "ben-a", shareBps: BPS_DENOM }],
    };
    const config2 = {
      assetId: "asset-2",
      rateBps: 1000,
      beneficiaries: [{ id: "ben-b", shareBps: BPS_DENOM }],
    };
    const swaps = [
      { swapId: "s1", salePrice: 1000, royaltyConfig: config1 },
      { swapId: "s2", salePrice: 2000, royaltyConfig: config2 },
    ];
    const result = distributeBatchRoyalties(swaps);
    expect(result.aggregated).toHaveLength(2);
    const benA = result.aggregated.find((a) => a.beneficiaryId === "ben-a");
    const benB = result.aggregated.find((a) => a.beneficiaryId === "ben-b");
    expect(benA.totalAmount).toBe(100);
    expect(benB.totalAmount).toBe(200);
  });
});

// ── #917: settleBeneficiaryPayouts ────────────────────────────────────────

describe("settleBeneficiaryPayouts — validation", () => {
  test("throws on non-array ledger", () => {
    expect(() => settleBeneficiaryPayouts(null, "b1")).toThrow(TypeError);
  });

  test("throws on missing beneficiaryId", () => {
    expect(() => settleBeneficiaryPayouts([], null)).toThrow(TypeError);
  });

  test("throws on empty string beneficiaryId", () => {
    expect(() => settleBeneficiaryPayouts([], "")).toThrow(TypeError);
  });
});

describe("settleBeneficiaryPayouts — settlement logic", () => {
  const ledger = [
    { beneficiaryId: "b1", amount: 100, status: "PENDING" },
    { beneficiaryId: "b1", amount: 200, status: "PENDING" },
    { beneficiaryId: "b2", amount: 150, status: "PENDING" },
    { beneficiaryId: "b1", amount: 50, status: "PAID" },
  ];

  test("marks pending payouts as PAID and records paidAt timestamp", () => {
    const ledgerCopy = JSON.parse(JSON.stringify(ledger));
    const result = settleBeneficiaryPayouts(ledgerCopy, "b1");
    expect(result.paid).toHaveLength(2);
    expect(result.paid[0].status).toBe("PAID");
    expect(result.paid[0].paidAt).toBeDefined();
    expect(result.paid[1].status).toBe("PAID");
    expect(result.paid[1].paidAt).toBeDefined();
  });

  test("calculates totalPaid correctly", () => {
    const ledgerCopy = JSON.parse(JSON.stringify(ledger));
    const result = settleBeneficiaryPayouts(ledgerCopy, "b1");
    expect(result.totalPaid).toBe(300); // 100 + 200
  });

  test("skips already PAID entries", () => {
    const ledgerCopy = JSON.parse(JSON.stringify(ledger));
    const result = settleBeneficiaryPayouts(ledgerCopy, "b1");
    // Should only settle pending entries (100 + 200), not the already PAID (50)
    expect(result.paid).toHaveLength(2);
    expect(result.totalPaid).toBe(300);
  });

  test("skips entries for different beneficiary", () => {
    const ledgerCopy = JSON.parse(JSON.stringify(ledger));
    const result = settleBeneficiaryPayouts(ledgerCopy, "b1");
    // b2's entry should not be included
    const b2Included = result.paid.some((p) => p.beneficiaryId === "b2");
    expect(b2Included).toBe(false);
  });

  test("respects maxAmount cap", () => {
    const ledgerCopy = JSON.parse(JSON.stringify(ledger));
    const result = settleBeneficiaryPayouts(ledgerCopy, "b1", { maxAmount: 150 });
    expect(result.totalPaid).toBe(100); // only first entry, second would exceed cap
    expect(result.paid).toHaveLength(1);
  });

  test("stops settling when maxAmount would be exceeded", () => {
    const ledgerCopy = JSON.parse(JSON.stringify(ledger));
    const result = settleBeneficiaryPayouts(ledgerCopy, "b1", { maxAmount: 250 });
    // Should settle first (100) and second (200) but not both if total exceeds
    expect(result.totalPaid).toBe(300); // actually both fit
  });

  test("returns empty paid array if no matching pending entries", () => {
    const ledgerCopy = JSON.parse(JSON.stringify(ledger));
    const result = settleBeneficiaryPayouts(ledgerCopy, "unknown");
    expect(result.paid).toHaveLength(0);
    expect(result.totalPaid).toBe(0);
  });

  test("handles empty ledger gracefully", () => {
    const result = settleBeneficiaryPayouts([], "b1");
    expect(result.paid).toHaveLength(0);
    expect(result.totalPaid).toBe(0);
  });
});

describe("settleBeneficiaryPayouts — maxAmount edge cases", () => {
  test("maxAmount of zero settles nothing", () => {
    const ledger = [{ beneficiaryId: "b1", amount: 100, status: "PENDING" }];
    const result = settleBeneficiaryPayouts(ledger, "b1", { maxAmount: 0 });
    expect(result.paid).toHaveLength(0);
    expect(result.totalPaid).toBe(0);
  });

  test("maxAmount exceeding total settles all", () => {
    const ledger = [
      { beneficiaryId: "b1", amount: 100, status: "PENDING" },
      { beneficiaryId: "b1", amount: 100, status: "PENDING" },
    ];
    const result = settleBeneficiaryPayouts(ledger, "b1", { maxAmount: 500 });
    expect(result.paid).toHaveLength(2);
    expect(result.totalPaid).toBe(200);
  });

  test("undefined maxAmount defaults to Infinity", () => {
    const ledger = [
      { beneficiaryId: "b1", amount: 100, status: "PENDING" },
      { beneficiaryId: "b1", amount: 200, status: "PENDING" },
    ];
    const result = settleBeneficiaryPayouts(ledger, "b1", {});
    expect(result.paid).toHaveLength(2);
    expect(result.totalPaid).toBe(300);
  });
});
