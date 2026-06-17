/**
 * Swap Batch Pricing — Issue #512
 * ────────────────────────────────
 * Dynamic pricing for batch swaps.
 *
 * Supports:
 *  - Volume-tiered pricing (bulk discounts)
 *  - Per-swap price overrides
 *  - Oracle-based price feeds (pluggable)
 *  - Price floor / ceiling guards
 *  - Batch price summary with effective rates
 */

const BPS_DENOM      = 10_000;
const MAX_BATCH_SIZE = 100;

// ── Default pricing tiers (discount in BPS off base price) ───────────────────
const DEFAULT_VOLUME_TIERS = [
  { minCount: 1,  discountBps: 0   }, // no discount
  { minCount: 5,  discountBps: 50  }, // 0.5% off
  { minCount: 10, discountBps: 100 }, // 1.0% off
  { minCount: 25, discountBps: 200 }, // 2.0% off
  { minCount: 50, discountBps: 300 }, // 3.0% off
];

// ── Validation ────────────────────────────────────────────────────────────────

function validateSwapPriceEntry(entry, index) {
  if (!entry || typeof entry !== "object")
    throw new TypeError(`Entry at index ${index} must be an object.`);
  if (!entry.swapId)
    throw new TypeError(`Entry at index ${index}: swapId is required.`);
  if (typeof entry.basePrice !== "number" || entry.basePrice <= 0)
    throw new RangeError(`Entry at index ${index}: basePrice must be a positive number.`);
}

function validatePriceBounds(price, floor, ceiling, swapId) {
  if (floor > 0 && price < floor)
    throw new RangeError(`Swap ${swapId}: price ${price} is below floor ${floor}.`);
  if (ceiling > 0 && price > ceiling)
    throw new RangeError(`Swap ${swapId}: price ${price} exceeds ceiling ${ceiling}.`);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/**
 * Resolve the volume-tier discount for a given batch count.
 *
 * @param {number} count
 * @param {object[]} [tiers]
 * @returns {number} discountBps
 */
function resolveVolumeTierDiscount(count, tiers = DEFAULT_VOLUME_TIERS) {
  let discountBps = 0;
  for (const tier of tiers) {
    if (count >= tier.minCount) discountBps = tier.discountBps;
  }
  return discountBps;
}

/**
 * Apply a discount (in BPS) to a price.
 *
 * @param {number} price
 * @param {number} discountBps
 * @returns {number}
 */
function applyDiscount(price, discountBps) {
  return price - Math.floor((price * discountBps) / BPS_DENOM);
}

// ── Core ──────────────────────────────────────────────────────────────────────

/**
 * Calculate dynamic prices for a batch of swaps.
 *
 * @param {Array<{ swapId, basePrice, overridePrice?: number }>} swaps
 * @param {object} [options]
 * @param {number}   [options.priceFloor=0]       - minimum allowed price (0 = no floor)
 * @param {number}   [options.priceCeiling=0]     - maximum allowed price (0 = no ceiling)
 * @param {boolean}  [options.applyVolumeTier=true]
 * @param {object[]} [options.volumeTiers]        - custom tier table
 * @param {Function} [options.oracleFn]           - async/sync fn(swapId) → price override
 * @returns {BatchPricingResult}
 *
 * @typedef {Object} SwapPriceEntry
 * @property {string|number} swapId
 * @property {number}        basePrice
 * @property {number}        discountBps
 * @property {number}        discountAmount
 * @property {number}        finalPrice
 * @property {string}        priceSource   - "override" | "oracle" | "volume_tier" | "base"
 *
 * @typedef {Object} BatchPricingResult
 * @property {number}          batchSize
 * @property {number}          volumeDiscountBps
 * @property {number}          totalBaseValue
 * @property {number}          totalFinalValue
 * @property {number}          totalDiscount
 * @property {SwapPriceEntry[]} prices
 */
function calculateBatchPrices(swaps, options = {}) {
  if (!Array.isArray(swaps) || swaps.length === 0)
    throw new TypeError("swaps must be a non-empty array.");
  if (swaps.length > MAX_BATCH_SIZE)
    throw new RangeError(`Batch size ${swaps.length} exceeds maximum of ${MAX_BATCH_SIZE}.`);

  swaps.forEach(validateSwapPriceEntry);

  const {
    priceFloor      = 0,
    priceCeiling    = 0,
    applyVolumeTier = true,
    volumeTiers     = DEFAULT_VOLUME_TIERS,
    oracleFn        = null,
  } = options;

  const volumeDiscountBps = applyVolumeTier
    ? resolveVolumeTierDiscount(swaps.length, volumeTiers)
    : 0;

  const prices = swaps.map((swap) => {
    let finalPrice;
    let priceSource;
    let discountBps    = 0;
    let discountAmount = 0;

    if (typeof swap.overridePrice === "number" && swap.overridePrice > 0) {
      finalPrice  = swap.overridePrice;
      priceSource = "override";
    } else if (oracleFn) {
      const oraclePrice = oracleFn(swap.swapId);
      if (typeof oraclePrice === "number" && oraclePrice > 0) {
        finalPrice  = oraclePrice;
        priceSource = "oracle";
      } else {
        finalPrice  = applyDiscount(swap.basePrice, volumeDiscountBps);
        discountBps = volumeDiscountBps;
        priceSource = "volume_tier";
      }
    } else if (volumeDiscountBps > 0) {
      finalPrice  = applyDiscount(swap.basePrice, volumeDiscountBps);
      discountBps = volumeDiscountBps;
      priceSource = "volume_tier";
    } else {
      finalPrice  = swap.basePrice;
      priceSource = "base";
    }

    discountAmount = swap.basePrice - finalPrice;
    validatePriceBounds(finalPrice, priceFloor, priceCeiling, swap.swapId);

    return {
      swapId:        swap.swapId,
      basePrice:     swap.basePrice,
      discountBps,
      discountAmount,
      finalPrice,
      priceSource,
    };
  });

  const totalBaseValue  = prices.reduce((s, p) => s + p.basePrice, 0);
  const totalFinalValue = prices.reduce((s, p) => s + p.finalPrice, 0);

  return {
    batchSize:        swaps.length,
    volumeDiscountBps,
    totalBaseValue,
    totalFinalValue,
    totalDiscount:    totalBaseValue - totalFinalValue,
    prices,
  };
}

/**
 * Apply a flat percentage markup/markdown to all prices in a batch.
 *
 * @param {Array<{ swapId, basePrice }>} swaps
 * @param {number} adjustmentBps  - positive = markup, negative = markdown
 * @returns {Array<{ swapId, basePrice, adjustedPrice, adjustmentBps }>}
 */
function applyBatchPriceAdjustment(swaps, adjustmentBps) {
  if (!Array.isArray(swaps) || swaps.length === 0)
    throw new TypeError("swaps must be a non-empty array.");
  if (typeof adjustmentBps !== "number")
    throw new TypeError("adjustmentBps must be a number.");

  return swaps.map((swap, i) => {
    validateSwapPriceEntry(swap, i);
    const delta         = Math.floor((swap.basePrice * Math.abs(adjustmentBps)) / BPS_DENOM);
    const adjustedPrice = adjustmentBps >= 0
      ? swap.basePrice + delta
      : Math.max(1, swap.basePrice - delta);
    return { swapId: swap.swapId, basePrice: swap.basePrice, adjustedPrice, adjustmentBps };
  });
}

/**
 * Validate that all prices in a batch are within allowed bounds.
 *
 * @param {Array<{ swapId, price }>} swaps
 * @param {{ floor?: number, ceiling?: number }} bounds
 * @returns {{ valid: object[], invalid: object[] }}
 */
function validateBatchPriceBounds(swaps, bounds = {}) {
  if (!Array.isArray(swaps)) throw new TypeError("swaps must be an array.");

  const { floor = 0, ceiling = 0 } = bounds;
  const valid   = [];
  const invalid = [];

  for (const swap of swaps) {
    const violations = [];
    if (floor > 0 && swap.price < floor)     violations.push(`below floor ${floor}`);
    if (ceiling > 0 && swap.price > ceiling) violations.push(`above ceiling ${ceiling}`);

    if (violations.length === 0) {
      valid.push(swap);
    } else {
      invalid.push({ swapId: swap.swapId, price: swap.price, violations });
    }
  }

  return { valid, invalid };
}

module.exports = {
  calculateBatchPrices,
  applyBatchPriceAdjustment,
  validateBatchPriceBounds,
  resolveVolumeTierDiscount,
  applyDiscount,
  DEFAULT_VOLUME_TIERS,
  MAX_BATCH_SIZE,
  BPS_DENOM,
};
