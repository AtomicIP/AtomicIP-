/**
 * #879 — Royalty Calculation Consistency Test
 * ────────────────────────────────────────────
 * Asserts that swapRoyaltyTracker.js and batchRoyaltyDistributor.js
 * produce identical royalty amounts for identical inputs.
 *
 * Both modules now delegate to src/royalty/royaltyCalculation.js
 * (the canonical shared implementation) so this test also acts as
 * a regression guard: if either module diverges from the shared
 * formula in the future, these tests will catch it immediately.
 */

"use strict";

const { calculateRoyalty }    = require("../royalty/swapRoyaltyTracker");
const { calculateSwapRoyalty } = require("../batch/batchRoyaltyDistributor");
const { computeRoyaltyPayouts, BPS_DENOM } = require("../royalty/royaltyCalculation");

// ── Shared test fixtures ───────────────────────────────────────────────────────

const SINGLE_BENEFICIARY_CONFIG = {
  assetId:      "IP-001",
  rateBps:      500, // 5%
  beneficiaries: [{ id: "creator-1", shareBps: BPS_DENOM }],
};

const MULTI_BENEFICIARY_CONFIG = {
  assetId:      "IP-002",
  rateBps:      1000, // 10%
  beneficiaries: [
    { id: "creator-1", shareBps: 6000 }, // 60%
    { id: "creator-2", shareBps: 4000 }, // 40%
  ],
};

const DUST_EDGE_CONFIG = {
  assetId:      "IP-003",
  rateBps:      333, // 3.33% — produces dust in per-beneficiary split
  beneficiaries: [
    { id: "creator-1", shareBps: 3333 },
    { id: "creator-2", shareBps: 3333 },
    { id: "creator-3", shareBps: 3334 },
  ],
};

// ── Helper: extract the batchRoyaltyDistributor result in a comparable shape ──

function batchCalc(config, salePrice) {
  return calculateSwapRoyalty("swap-test", salePrice, config);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("#879 — swapRoyaltyTracker and batchRoyaltyDistributor agree: single beneficiary", () => {
  const SALE_PRICES = [100, 1000, 9999, 12345, 1_000_000];

  for (const price of SALE_PRICES) {
    test(`salePrice=${price}: totalRoyalty and sellerProceeds match`, () => {
      const tracker = calculateRoyalty(SINGLE_BENEFICIARY_CONFIG, price);
      const batch   = batchCalc(SINGLE_BENEFICIARY_CONFIG, price);

      expect(batch.totalRoyalty).toBe(tracker.totalRoyalty);
      expect(batch.sellerProceeds).toBe(tracker.sellerProceeds);
    });

    test(`salePrice=${price}: per-beneficiary amounts match`, () => {
      const tracker = calculateRoyalty(SINGLE_BENEFICIARY_CONFIG, price);
      const batch   = batchCalc(SINGLE_BENEFICIARY_CONFIG, price);

      expect(batch.payouts).toHaveLength(tracker.payouts.length);
      for (let i = 0; i < tracker.payouts.length; i++) {
        expect(batch.payouts[i].amount).toBe(tracker.payouts[i].amount);
        expect(batch.payouts[i].beneficiaryId).toBe(tracker.payouts[i].beneficiaryId);
      }
    });
  }
});

describe("#879 — swapRoyaltyTracker and batchRoyaltyDistributor agree: multiple beneficiaries", () => {
  const SALE_PRICES = [500, 3333, 10_000, 50_001];

  for (const price of SALE_PRICES) {
    test(`salePrice=${price}: totalRoyalty matches`, () => {
      const tracker = calculateRoyalty(MULTI_BENEFICIARY_CONFIG, price);
      const batch   = batchCalc(MULTI_BENEFICIARY_CONFIG, price);
      expect(batch.totalRoyalty).toBe(tracker.totalRoyalty);
    });

    test(`salePrice=${price}: all per-beneficiary amounts match`, () => {
      const tracker = calculateRoyalty(MULTI_BENEFICIARY_CONFIG, price);
      const batch   = batchCalc(MULTI_BENEFICIARY_CONFIG, price);

      for (let i = 0; i < tracker.payouts.length; i++) {
        expect(batch.payouts[i].amount).toBe(tracker.payouts[i].amount);
      }
    });
  }
});

describe("#879 — Dust assignment: first-beneficiary rule is consistent across modules", () => {
  const SALE_PRICES = [1, 3, 7, 99, 301, 10_000];

  for (const price of SALE_PRICES) {
    test(`salePrice=${price}: dust (if any) goes to first beneficiary in both modules`, () => {
      const tracker = calculateRoyalty(DUST_EDGE_CONFIG, price);
      const batch   = batchCalc(DUST_EDGE_CONFIG, price);

      // Both must assign identical dust to beneficiary[0]
      expect(batch.payouts[0].amount).toBe(tracker.payouts[0].amount);

      // In both modules: sum of payouts === totalRoyalty
      const trackerSum = tracker.payouts.reduce((s, p) => s + p.amount, 0);
      const batchSum   = batch.payouts.reduce((s, p) => s + p.amount, 0);
      expect(trackerSum).toBe(tracker.totalRoyalty);
      expect(batchSum).toBe(batch.totalRoyalty);
    });
  }
});

describe("#879 — Canonical shared formula (royaltyCalculation.js) matches both modules", () => {
  test("shared computeRoyaltyPayouts produces same result as swapRoyaltyTracker", () => {
    const config     = MULTI_BENEFICIARY_CONFIG;
    const salePrice  = 12_345;
    const tracker    = calculateRoyalty(config, salePrice);
    const canonical  = computeRoyaltyPayouts(salePrice, config.rateBps, config.beneficiaries);

    expect(canonical.totalRoyalty).toBe(tracker.totalRoyalty);
    expect(canonical.sellerProceeds).toBe(tracker.sellerProceeds);
    for (let i = 0; i < tracker.payouts.length; i++) {
      expect(canonical.payouts[i].amount).toBe(tracker.payouts[i].amount);
    }
  });

  test("shared computeRoyaltyPayouts produces same result as batchRoyaltyDistributor", () => {
    const config     = MULTI_BENEFICIARY_CONFIG;
    const salePrice  = 12_345;
    const batch      = batchCalc(config, salePrice);
    const canonical  = computeRoyaltyPayouts(salePrice, config.rateBps, config.beneficiaries);

    expect(canonical.totalRoyalty).toBe(batch.totalRoyalty);
    expect(canonical.sellerProceeds).toBe(batch.sellerProceeds);
    for (let i = 0; i < batch.payouts.length; i++) {
      expect(canonical.payouts[i].amount).toBe(batch.payouts[i].amount);
    }
  });

  test("zero rate: totalRoyalty is 0 in both modules", () => {
    const config = { ...SINGLE_BENEFICIARY_CONFIG, rateBps: 0 };
    const tracker = calculateRoyalty(config, 50_000);
    const batch   = batchCalc(config, 50_000);

    expect(tracker.totalRoyalty).toBe(0);
    expect(batch.totalRoyalty).toBe(0);
  });

  test("max rate (3000 bps = 30%): both modules agree", () => {
    const config = { ...SINGLE_BENEFICIARY_CONFIG, rateBps: 3000 };
    const price  = 100_000;
    const tracker = calculateRoyalty(config, price);
    const batch   = batchCalc(config, price);

    expect(tracker.totalRoyalty).toBe(batch.totalRoyalty);
    expect(tracker.totalRoyalty).toBe(30_000); // 30% of 100_000
  });

  test("canonical formula: totalRoyalty === floor(salePrice × rateBps / 10000)", () => {
    const cases = [
      { price: 100,    rateBps: 500  },
      { price: 333,    rateBps: 1000 },
      { price: 9999,   rateBps: 250  },
      { price: 10_000, rateBps: 3000 },
    ];

    for (const { price, rateBps } of cases) {
      const config   = { ...SINGLE_BENEFICIARY_CONFIG, rateBps };
      const tracker  = calculateRoyalty(config, price);
      const expected = Math.floor((price * rateBps) / BPS_DENOM);
      expect(tracker.totalRoyalty).toBe(expected);
    }
  });
});
