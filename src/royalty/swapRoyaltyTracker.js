/**
 * Swap Royalty Tracking — Issue #472
 * ─────────────────────────────────────
 * Tracks and distributes royalties from IP resales.
 *
 * Model:
 *  - Each IP asset has a royalty configuration: beneficiary(s) + rate
 *  - Every resale swap triggers a royalty calculation
 *  - Royalties split pro-rata among multiple beneficiaries
 *  - Distribution ledger tracks pending and paid royalties
 */

const { computeRoyaltyPayouts, BPS_DENOM: CANONICAL_BPS_DENOM, MAX_ROYALTY_RATE_BPS: CANONICAL_MAX_RATE_BPS } = require("./royaltyCalculation");

const MAX_ROYALTY_RATE_BPS = CANONICAL_MAX_RATE_BPS;
const BPS_DENOM = CANONICAL_BPS_DENOM;
const MAX_BENEFICIARIES = 10;

function validateRoyaltyConfig(config) {
  if (!config || typeof config !== "object")
    throw new TypeError("royaltyConfig must be an object.");
  if (!config.assetId)
    throw new TypeError("royaltyConfig: assetId is required.");
  if (typeof config.rateBps !== "number" || config.rateBps < 0 || config.rateBps > MAX_ROYALTY_RATE_BPS)
    throw new RangeError(`rateBps must be between 0 and ${MAX_ROYALTY_RATE_BPS}.`);
  if (!Array.isArray(config.beneficiaries) || config.beneficiaries.length === 0)
    throw new TypeError("beneficiaries must be a non-empty array.");
  if (config.beneficiaries.length > MAX_BENEFICIARIES)
    throw new RangeError(`Maximum ${MAX_BENEFICIARIES} beneficiaries per asset.`);

  const totalShare = config.beneficiaries.reduce((s, b) => s + (b.shareBps ?? 0), 0);
  if (Math.abs(totalShare - BPS_DENOM) > 1)
    throw new RangeError(`Beneficiary shares must sum to ${BPS_DENOM} bps (got ${totalShare}).`);
}

function calculateRoyalty(config, salePrice) {
  validateRoyaltyConfig(config);
  if (typeof salePrice !== "number" || salePrice <= 0)
    throw new RangeError("salePrice must be a positive number.");

  const { totalRoyalty, sellerProceeds, payouts } = computeRoyaltyPayouts(
    salePrice,
    config.rateBps,
    config.beneficiaries
  );

  return {
    assetId: config.assetId,
    salePrice,
    totalRoyalty,
    rateBps: config.rateBps,
    payouts,
    sellerProceeds,
  };
}

function recordRoyaltyEvent(ledger, swapId, calculation) {
  if (!Array.isArray(ledger)) throw new TypeError("ledger must be an array.");
  if (!swapId) throw new TypeError("swapId is required.");

  const createdAt = new Date().toISOString();
  const entries = calculation.payouts.map((p, i) => ({
    entryId: `${swapId}-${calculation.assetId}-${i}`,
    swapId,
    assetId: calculation.assetId,
    beneficiaryId: p.beneficiaryId,
    amount: p.amount,
    status: "PENDING",
    createdAt,
  }));

  ledger.push(...entries);
  return entries;
}

function processPayouts(ledger, beneficiaryId, options = {}) {
  if (!beneficiaryId) throw new TypeError("beneficiaryId is required.");
  const maxAmount = options.maxAmount ?? Infinity;

  let remaining = maxAmount;
  const paid = [];

  for (const entry of ledger) {
    if (entry.beneficiaryId !== beneficiaryId) continue;
    if (entry.status !== "PENDING") continue;
    if (Number.isFinite(maxAmount) && entry.amount > maxAmount) break;

    entry.status = "PAID";
    entry.paidAt = new Date().toISOString();
    paid.push(entry);
    remaining -= entry.amount;
  }

  return { paid, totalPaid: paid.reduce((s, e) => s + e.amount, 0) };
}

function getPendingRoyalties(ledger, beneficiaryId) {
  const entries = ledger.filter(
    (e) => e.beneficiaryId === beneficiaryId && e.status === "PENDING"
  );
  return { beneficiaryId, entries, total: entries.reduce((s, e) => s + e.amount, 0) };
}

function processBatchRoyalties(transactions, ledger) {
  if (!Array.isArray(transactions) || transactions.length === 0)
    throw new TypeError("transactions must be a non-empty array.");

  const results = [];
  const errors = [];

  for (const tx of transactions) {
    try {
      const calc = calculateRoyalty(tx.config, tx.salePrice);
      const entries = recordRoyaltyEvent(ledger, tx.swapId, calc);
      results.push({ swapId: tx.swapId, calculation: calc, entries });
    } catch (err) {
      errors.push({ swapId: tx.swapId, error: err.message });
    }
  }

  return {
    processed: results.length,
    failed: errors.length,
    totalRoyaltiesGenerated: results.reduce((s, r) => s + r.calculation.totalRoyalty, 0),
    results,
    errors,
  };
}

module.exports = {
  calculateRoyalty,
  recordRoyaltyEvent,
  processPayouts,
  getPendingRoyalties,
  processBatchRoyalties,
  validateRoyaltyConfig,
  MAX_ROYALTY_RATE_BPS,
  BPS_DENOM,
};
