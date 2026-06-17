/**
 * Swap Batch Fee Calculation
 * ─────────────────────────
 * Calculates fees for a batch of swaps with support for:
 *  - Per-swap base fee
 *  - Volume-tiered fee rates
 *  - Batch discount (economies of scale)
 *  - Protocol fee split (platform vs liquidity providers)
 */

// ── Fee rate tiers (basis points) ────────────────────────────────────────────
const FEE_TIERS = [
  { minVolume: 0,          bps: 30  }, // 0.30% — default
  { minVolume: 10_000,     bps: 25  }, // 0.25% — mid tier
  { minVolume: 100_000,    bps: 20  }, // 0.20% — high volume
  { minVolume: 1_000_000,  bps: 15  }, // 0.15% — institutional
];

const BASE_FEE_PER_SWAP   = 0.001;  // flat fee per swap entry (in token units)
const BATCH_DISCOUNT_BPS  = 5;      // 0.05% discount applied to total fee for batch
const PROTOCOL_SPLIT_BPS  = 2000;   // 20% of fee goes to protocol treasury
const MAX_BATCH_SIZE       = 100;
const BPS_DENOM            = 10_000;

// ── Helpers ───────────────────────────────────────────────────────────────────

function getTierRate(volume) {
  let bps = FEE_TIERS[0].bps;
  for (const tier of FEE_TIERS) {
    if (volume >= tier.minVolume) bps = tier.bps;
  }
  return bps;
}

function validateSwap(swap, index) {
  if (!swap || typeof swap !== "object")
    throw new TypeError(`Swap at index ${index} must be an object.`);
  if (typeof swap.amount !== "number" || swap.amount <= 0)
    throw new RangeError(`Swap at index ${index}: amount must be a positive number.`);
  if (typeof swap.value !== "number" || swap.value <= 0)
    throw new RangeError(`Swap at index ${index}: value must be a positive number.`);
}

// ── Core calculation ──────────────────────────────────────────────────────────

/**
 * Calculate fees for a batch of swaps.
 *
 * @param {Array<{ id?: string, amount: number, value: number }>} swaps
 * @param {{ overrideFeeBps?: number, applyBatchDiscount?: boolean }} [options]
 * @returns {BatchFeeResult}
 *
 * @typedef {Object} SwapFee
 * @property {string|number} id
 * @property {number} amount
 * @property {number} value
 * @property {number} feeBps
 * @property {number} grossFee      - volume-rate fee + base fee
 * @property {number} discountAmount
 * @property {number} netFee
 * @property {number} protocolFee
 * @property {number} lpFee
 *
 * @typedef {Object} BatchFeeResult
 * @property {number} batchSize
 * @property {number} totalVolume
 * @property {number} effectiveFeeBps
 * @property {number} totalGrossFee
 * @property {number} totalDiscount
 * @property {number} totalNetFee
 * @property {number} totalProtocolFee
 * @property {number} totalLpFee
 * @property {SwapFee[]} swapFees
 */
function calculateBatchFees(swaps, options = {}) {
  if (!Array.isArray(swaps) || swaps.length === 0)
    throw new TypeError("swaps must be a non-empty array.");
  if (swaps.length > MAX_BATCH_SIZE)
    throw new RangeError(`Batch size ${swaps.length} exceeds maximum of ${MAX_BATCH_SIZE}.`);

  swaps.forEach(validateSwap);

  const { overrideFeeBps, applyBatchDiscount = true } = options;
  const totalVolume = swaps.reduce((s, sw) => s + sw.value, 0);
  const tierBps = overrideFeeBps ?? getTierRate(totalVolume);

  const swapFees = swaps.map((swap, i) => {
    const volumeFee = (swap.value * tierBps) / BPS_DENOM;
    const grossFee  = volumeFee + BASE_FEE_PER_SWAP;

    const discountAmount = applyBatchDiscount
      ? (grossFee * BATCH_DISCOUNT_BPS) / BPS_DENOM
      : 0;

    const netFee      = grossFee - discountAmount;
    const protocolFee = (netFee * PROTOCOL_SPLIT_BPS) / BPS_DENOM;
    const lpFee       = netFee - protocolFee;

    return {
      id:             swap.id ?? i,
      amount:         swap.amount,
      value:          swap.value,
      feeBps:         tierBps,
      grossFee:       +grossFee.toFixed(8),
      discountAmount: +discountAmount.toFixed(8),
      netFee:         +netFee.toFixed(8),
      protocolFee:    +protocolFee.toFixed(8),
      lpFee:          +lpFee.toFixed(8),
    };
  });

  const totalGrossFee    = swapFees.reduce((s, f) => s + f.grossFee, 0);
  const totalDiscount    = swapFees.reduce((s, f) => s + f.discountAmount, 0);
  const totalNetFee      = swapFees.reduce((s, f) => s + f.netFee, 0);
  const totalProtocolFee = swapFees.reduce((s, f) => s + f.protocolFee, 0);
  const totalLpFee       = swapFees.reduce((s, f) => s + f.lpFee, 0);

  return {
    batchSize:        swaps.length,
    totalVolume:      +totalVolume.toFixed(8),
    effectiveFeeBps:  tierBps,
    totalGrossFee:    +totalGrossFee.toFixed(8),
    totalDiscount:    +totalDiscount.toFixed(8),
    totalNetFee:      +totalNetFee.toFixed(8),
    totalProtocolFee: +totalProtocolFee.toFixed(8),
    totalLpFee:       +totalLpFee.toFixed(8),
    swapFees,
  };
}

module.exports = {
  calculateBatchFees,
  getTierRate,
  FEE_TIERS,
  BASE_FEE_PER_SWAP,
  BATCH_DISCOUNT_BPS,
  PROTOCOL_SPLIT_BPS,
  MAX_BATCH_SIZE,
};
