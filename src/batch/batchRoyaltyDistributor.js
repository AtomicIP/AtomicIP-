/**
 * Swap Batch Royalty Distribution — Issue #511
 * ─────────────────────────────────────────────
 * Distributes royalties for a batch of completed swaps.
 *
 * Each swap may reference an IP asset with a royalty config.
 * Royalties are calculated per-swap and aggregated per beneficiary
 * so a single payout pass can settle all obligations.
 */

const MAX_ROYALTY_RATE_BPS = 3000; // 30% ceiling
const BPS_DENOM            = 10_000;
const MAX_BATCH_SIZE       = 100;
const MAX_BENEFICIARIES    = 10;

// ── Validation ────────────────────────────────────────────────────────────────

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

// ── Core ──────────────────────────────────────────────────────────────────────

/**
 * Calculate royalty payouts for a single swap.
 *
 * @param {string|number} swapId
 * @param {number}        salePrice
 * @param {object}        royaltyConfig  { assetId, rateBps, beneficiaries: [{ id, shareBps }] }
 * @returns {object}
 */
function calculateSwapRoyalty(swapId, salePrice, royaltyConfig) {
  const totalRoyalty = Math.floor((salePrice * royaltyConfig.rateBps) / BPS_DENOM);

  const payouts = royaltyConfig.beneficiaries.map((b) => ({
    beneficiaryId: b.id,
    shareBps:      b.shareBps,
    amount:        Math.floor((totalRoyalty * b.shareBps) / BPS_DENOM),
  }));

  // Assign rounding dust to first beneficiary
  const distributed = payouts.reduce((s, p) => s + p.amount, 0);
  if (distributed < totalRoyalty) payouts[0].amount += totalRoyalty - distributed;

  return {
    swapId,
    assetId:        royaltyConfig.assetId,
    salePrice,
    rateBps:        royaltyConfig.rateBps,
    totalRoyalty,
    sellerProceeds: salePrice - totalRoyalty,
    payouts,
  };
}

/**
 * Distribute royalties for a batch of completed swaps.
 *
 * @param {Array<{ swapId, salePrice, royaltyConfig }>} swaps
 * @returns {BatchRoyaltyResult}
 *
 * @typedef {Object} BatchRoyaltyResult
 * @property {number}   batchSize
 * @property {number}   processed
 * @property {number}   failed
 * @property {number}   totalRoyaltiesGenerated
 * @property {object[]} distributions   - per-swap royalty breakdowns
 * @property {object[]} aggregated      - royalties summed per beneficiary
 * @property {object[]} errors
 */
function distributeBatchRoyalties(swaps) {
  if (!Array.isArray(swaps) || swaps.length === 0)
    throw new TypeError("swaps must be a non-empty array.");
  if (swaps.length > MAX_BATCH_SIZE)
    throw new RangeError(`Batch size ${swaps.length} exceeds maximum of ${MAX_BATCH_SIZE}.`);

  swaps.forEach(validateSwapEntry);

  const distributions = [];
  const errors        = [];

  for (let i = 0; i < swaps.length; i++) {
    const { swapId, salePrice, royaltyConfig } = swaps[i];
    try {
      distributions.push(calculateSwapRoyalty(swapId, salePrice, royaltyConfig));
    } catch (err) {
      errors.push({ swapId, error: err.message });
    }
  }

  // Aggregate per beneficiary across all swaps
  const beneficiaryMap = new Map();
  for (const dist of distributions) {
    for (const payout of dist.payouts) {
      const existing = beneficiaryMap.get(payout.beneficiaryId) ?? { beneficiaryId: payout.beneficiaryId, totalAmount: 0, swapCount: 0 };
      existing.totalAmount += payout.amount;
      existing.swapCount   += 1;
      beneficiaryMap.set(payout.beneficiaryId, existing);
    }
  }

  return {
    batchSize:               swaps.length,
    processed:               distributions.length,
    failed:                  errors.length,
    totalRoyaltiesGenerated: distributions.reduce((s, d) => s + d.totalRoyalty, 0),
    distributions,
    aggregated:              Array.from(beneficiaryMap.values()),
    errors,
  };
}

/**
 * Mark royalties as paid for a specific beneficiary up to an optional cap.
 *
 * @param {object[]} ledger
 * @param {string}   beneficiaryId
 * @param {{ maxAmount?: number }} [options]
 * @returns {{ paid: object[], totalPaid: number }}
 */
function settleBeneficiaryPayouts(ledger, beneficiaryId, options = {}) {
  if (!Array.isArray(ledger)) throw new TypeError("ledger must be an array.");
  if (!beneficiaryId)         throw new TypeError("beneficiaryId is required.");

  const maxAmount = options.maxAmount ?? Infinity;
  let remaining   = maxAmount;
  const paid      = [];

  for (const entry of ledger) {
    if (entry.beneficiaryId !== beneficiaryId) continue;
    if (entry.status !== "PENDING")            continue;
    if (entry.amount > remaining)              break;

    entry.status = "PAID";
    entry.paidAt = new Date().toISOString();
    paid.push(entry);
    remaining -= entry.amount;
  }

  return { paid, totalPaid: paid.reduce((s, e) => s + e.amount, 0) };
}

/**
 * Record batch royalty distributions to a ledger.
 *
 * @param {object[]} ledger
 * @param {object[]} distributions  - output of distributeBatchRoyalties().distributions
 * @returns {object[]} new ledger entries
 */
function recordBatchToLedger(ledger, distributions) {
  if (!Array.isArray(ledger))        throw new TypeError("ledger must be an array.");
  if (!Array.isArray(distributions)) throw new TypeError("distributions must be an array.");

  const createdAt = new Date().toISOString();
  const entries   = [];

  for (const dist of distributions) {
    for (let i = 0; i < dist.payouts.length; i++) {
      const p = dist.payouts[i];
      const entry = {
        entryId:       `${dist.swapId}-${dist.assetId}-${i}`,
        swapId:        dist.swapId,
        assetId:       dist.assetId,
        beneficiaryId: p.beneficiaryId,
        amount:        p.amount,
        status:        "PENDING",
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
