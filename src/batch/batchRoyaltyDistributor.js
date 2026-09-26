/**
 * Swap Batch Royalty Distribution — Issue #511
 * ─────────────────────────────────────────────
 * Distributes royalties for a batch of completed swaps.
 *
 * Each swap may reference an IP asset with a royalty config.
 * Royalties are calculated per-swap and aggregated per beneficiary
 * so a single payout pass can settle all obligations.
 */

const { computeRoyaltyPayouts, BPS_DENOM: CANONICAL_BPS_DENOM } = require("../royalty/royaltyCalculation");

const MAX_ROYALTY_RATE_BPS = 3000; // 30% ceiling
const BPS_DENOM = CANONICAL_BPS_DENOM;
const MAX_BATCH_SIZE = 100;
const MAX_BENEFICIARIES = 10;

function validateRoyaltyConfig(config, index) {
  if (!config || typeof config !== "object")
    throw new TypeError(`Swap at index ${index}: royaltyConfig must be an object.`);
  if (!config.assetId)
    throw new TypeError(`Swap at index ${index}: royaltyConfig.assetId is required.`);
  if (typeof config.rateBps !== "number" || config.rateBps < 0 || config.rateBps > MAX_ROYALTY_RATE_BPS)
    throw new RangeError(`Swap at index ${index}: rateBps must be 0–${MAX_ROYALTY_RATE_BPS}.`);
  if (!Array.isArray(config.beneficiaries) || config.beneficiaries.length === 0)
    throw new TypeError(`Swap at index ${index}: beneficiaries must be a non-empty array.`);
  if (config.beneficiaries.length > MAX_BENEFICIARIES)
    throw new RangeError(`Swap at index ${index}: max ${MAX_BENEFICIARIES} beneficiaries.`);

  const totalShare = config.beneficiaries.reduce((s, b) => s + (b.shareBps ?? 0), 0);
  if (Math.abs(totalShare - BPS_DENOM) > 1)
    throw new RangeError(`Swap at index ${index}: beneficiary shares must sum to ${BPS_DENOM} (got ${totalShare}).`);
}

function validateSwapEntry(swap, index) {
  if (!swap || typeof swap !== "object")
    throw new TypeError(`Entry at index ${index} must be an object.`);
  if (!swap.swapId)
    throw new TypeError(`Entry at index ${index}: swapId is required.`);
  if (typeof swap.salePrice !== "number" || swap.salePrice <= 0)
    throw new RangeError(`Entry at index ${index}: salePrice must be a positive number.`);
  validateRoyaltyConfig(swap.royaltyConfig, index);
}

function calculateSwapRoyalty(swapId, salePrice, royaltyConfig) {
  const { totalRoyalty, sellerProceeds, payouts } = computeRoyaltyPayouts(
    salePrice,
    royaltyConfig.rateBps,
    royaltyConfig.beneficiaries
  );

  return {
    swapId,
    assetId: royaltyConfig.assetId,
    salePrice,
    rateBps: royaltyConfig.rateBps,
    totalRoyalty,
    sellerProceeds,
    payouts,
  };
}

function distributeBatchRoyalties(swaps) {
  if (!Array.isArray(swaps) || swaps.length === 0)
    throw new TypeError("swaps must be a non-empty array.");
  if (swaps.length > MAX_BATCH_SIZE)
    throw new RangeError(`Batch size ${swaps.length} exceeds maximum of ${MAX_BATCH_SIZE}.`);

  const distributions = [];
  const errors = [];

  for (let i = 0; i < swaps.length; i++) {
    const swap = swaps[i];
    try {
      validateSwapEntry(swap, i);
      const result = calculateSwapRoyalty(swap.swapId, swap.salePrice, swap.royaltyConfig);
      distributions.push(result);
    } catch (err) {
      if (swaps.length === 1) throw err;
      errors.push({ swapId: swap?.swapId ?? `index-${i}`, error: err.message });
    }
  }

  const beneficiaryMap = new Map();
  for (const dist of distributions) {
    for (const payout of dist.payouts) {
      const existing = beneficiaryMap.get(payout.beneficiaryId) ?? { beneficiaryId: payout.beneficiaryId, totalAmount: 0, swapCount: 0 };
      existing.totalAmount += payout.amount;
      existing.swapCount += 1;
      beneficiaryMap.set(payout.beneficiaryId, existing);
    }
  }

  return {
    batchSize: swaps.length,
    processed: distributions.length,
    failed: errors.length,
    totalRoyaltiesGenerated: distributions.reduce((s, d) => s + d.totalRoyalty, 0),
    distributions,
    aggregated: Array.from(beneficiaryMap.values()),
    errors,
  };
}

function settleBeneficiaryPayouts(ledger, beneficiaryId, options = {}) {
  if (!Array.isArray(ledger)) throw new TypeError("ledger must be an array.");
  if (!beneficiaryId) throw new TypeError("beneficiaryId is required.");

  const maxAmount = options.maxAmount ?? Infinity;
  const paid = [];

  for (const entry of ledger) {
    if (entry.beneficiaryId !== beneficiaryId) continue;
    if (entry.status !== "PENDING") continue;
    if (Number.isFinite(maxAmount) && entry.amount > maxAmount) break;

    entry.status = "PAID";
    entry.paidAt = new Date().toISOString();
    paid.push(entry);
  }

  return { paid, totalPaid: paid.reduce((s, e) => s + e.amount, 0) };
}

function recordBatchToLedger(ledger, distributions) {
  if (!Array.isArray(ledger)) throw new TypeError("ledger must be an array.");
  if (!Array.isArray(distributions)) throw new TypeError("distributions must be an array.");

  const createdAt = new Date().toISOString();
  const entries = [];

  for (const dist of distributions) {
    for (let i = 0; i < dist.payouts.length; i++) {
      const p = dist.payouts[i];
      const entry = {
        entryId: `${dist.swapId}-${dist.assetId}-${i}`,
        swapId: dist.swapId,
        assetId: dist.assetId,
        beneficiaryId: p.beneficiaryId,
        amount: p.amount,
        status: "PENDING",
        createdAt,
      };
      ledger.push(entry);
      entries.push(entry);
    }
  }

  return entries;
}

module.exports = {
  distributeBatchRoyalties,
  calculateSwapRoyalty,
  settleBeneficiaryPayouts,
  recordBatchToLedger,
  validateRoyaltyConfig,
  MAX_ROYALTY_RATE_BPS,
  MAX_BATCH_SIZE,
  BPS_DENOM,
};
