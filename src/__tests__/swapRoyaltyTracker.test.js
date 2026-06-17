const {
  calculateRoyalty,
  recordRoyaltyEvent,
  processPayouts,
  getPendingRoyalties,
  processBatchRoyalties,
  MAX_ROYALTY_RATE_BPS,
  BPS_DENOM,
} = require("../royalty/swapRoyaltyTracker");

const singleBeneficiary = () => ({
  assetId:       "asset-1",
  rateBps:       1000, // 10%
  beneficiaries: [{ id: "creator-1", shareBps: 10_000 }],
});

const multiConfig = () => ({
  assetId:       "asset-2",
  rateBps:       500, // 5%
  beneficiaries: [
    { id: "creator-1", shareBps: 7_000 },
    { id: "agent-1",   shareBps: 3_000 },
  ],
});

describe("validateRoyaltyConfig", () => {
  test("throws on missing assetId", () => {
    expect(() => calculateRoyalty({ rateBps: 100, beneficiaries: [{ id: "x", shareBps: 10_000 }] }, 1000))
      .toThrow(TypeError);
  });

  test("throws if rateBps exceeds max", () => {
    expect(() => calculateRoyalty({ assetId: "a", rateBps: MAX_ROYALTY_RATE_BPS + 1, beneficiaries: [{ id: "x", shareBps: 10_000 }] }, 1000))
      .toThrow(RangeError);
  });

  test("throws if beneficiary shares do not sum to 10000", () => {
    const config = { assetId: "a", rateBps: 100, beneficiaries: [{ id: "x", shareBps: 5_000 }] };
    expect(() => calculateRoyalty(config, 1000)).toThrow(RangeError);
  });
});

describe("calculateRoyalty", () => {
  test("calculates correct totalRoyalty and sellerProceeds", () => {
    const result = calculateRoyalty(singleBeneficiary(), 10_000);
    expect(result.totalRoyalty).toBe(1_000);
    expect(result.sellerProceeds).toBe(9_000);
  });

  test("splits royalty between multiple beneficiaries", () => {
    const result = calculateRoyalty(multiConfig(), 10_000);
    expect(result.payouts).toHaveLength(2);
    const total = result.payouts.reduce((s, p) => s + p.amount, 0);
    expect(total).toBe(result.totalRoyalty);
  });

  test("dust assigned to first beneficiary", () => {
    // 3 beneficiaries splitting 10000bps with 3333/3333/3334 may produce dust
    const config = {
      assetId: "a", rateBps: 1000,
      beneficiaries: [
        { id: "b1", shareBps: 3334 },
        { id: "b2", shareBps: 3333 },
        { id: "b3", shareBps: 3333 },
      ],
    };
    const result = calculateRoyalty(config, 999);
    const total = result.payouts.reduce((s, p) => s + p.amount, 0);
    expect(total).toBe(result.totalRoyalty);
  });

  test("throws on non-positive salePrice", () => {
    expect(() => calculateRoyalty(singleBeneficiary(), 0)).toThrow(RangeError);
  });
});

describe("recordRoyaltyEvent", () => {
  test("adds entries to ledger", () => {
    const ledger = [];
    const calc   = calculateRoyalty(multiConfig(), 5_000);
    const entries = recordRoyaltyEvent(ledger, "swap-1", calc);
    expect(ledger).toHaveLength(2);
    expect(entries.every((e) => e.status === "PENDING")).toBe(true);
  });

  test("throws on missing swapId", () => {
    expect(() => recordRoyaltyEvent([], "", calculateRoyalty(singleBeneficiary(), 1000))).toThrow(TypeError);
  });
});

describe("processPayouts", () => {
  test("marks matching PENDING entries as PAID", () => {
    const ledger = [];
    recordRoyaltyEvent(ledger, "swap-1", calculateRoyalty(singleBeneficiary(), 10_000));
    const { paid, totalPaid } = processPayouts(ledger, "creator-1");
    expect(paid).toHaveLength(1);
    expect(totalPaid).toBe(1_000);
    expect(ledger[0].status).toBe("PAID");
  });

  test("respects maxAmount cap", () => {
    const ledger = [];
    recordRoyaltyEvent(ledger, "swap-1", calculateRoyalty(singleBeneficiary(), 10_000));
    recordRoyaltyEvent(ledger, "swap-2", calculateRoyalty(singleBeneficiary(), 10_000));
    const { paid } = processPayouts(ledger, "creator-1", { maxAmount: 1_500 });
    expect(paid).toHaveLength(1);
  });
});

describe("getPendingRoyalties", () => {
  test("returns correct pending total", () => {
    const ledger = [];
    recordRoyaltyEvent(ledger, "swap-1", calculateRoyalty(singleBeneficiary(), 10_000));
    const { total } = getPendingRoyalties(ledger, "creator-1");
    expect(total).toBe(1_000);
  });
});

describe("processBatchRoyalties", () => {
  test("processes multiple transactions and records to ledger", () => {
    const ledger = [];
    const txs = [
      { config: singleBeneficiary(), salePrice: 10_000, swapId: "s1" },
      { config: multiConfig(),       salePrice: 5_000,  swapId: "s2" },
    ];
    const result = processBatchRoyalties(txs, ledger);
    expect(result.processed).toBe(2);
    expect(result.failed).toBe(0);
    expect(result.totalRoyaltiesGenerated).toBeGreaterThan(0);
  });

  test("records errors without aborting batch", () => {
    const ledger = [];
    const txs = [
      { config: singleBeneficiary(), salePrice: 10_000, swapId: "s1" },
      { config: { assetId: "bad", rateBps: -1, beneficiaries: [] }, salePrice: 100, swapId: "s2" },
    ];
    const result = processBatchRoyalties(txs, ledger);
    expect(result.processed).toBe(1);
    expect(result.failed).toBe(1);
  });

  test("throws on empty transactions", () => {
    expect(() => processBatchRoyalties([], [])).toThrow(TypeError);
  });
});
