/**
 * Swap Batch Expiry Extension
 * ────────────────────────────
 * Extends the expiry timestamp for multiple swaps in one batch operation.
 *
 * Rules:
 *  - Only PENDING or ACTIVE swaps can be extended
 *  - Extension must be a positive duration
 *  - New expiry must not exceed MAX_EXPIRY_EXTENSION_MS from now
 *  - Already EXPIRED swaps are recorded as errors (non-throwing)
 */

const EXTENDABLE_STATES    = new Set(["PENDING", "ACTIVE"]);
const MAX_EXTENSION_MS     = 30 * 24 * 60 * 60 * 1000; // 30 days
const MIN_EXTENSION_MS     = 60 * 1000;                 // 1 minute
const MAX_BATCH_SIZE       = 100;

// ── Validators ────────────────────────────────────────────────────────────────

function validateSwap(swap, index) {
  if (!swap || typeof swap !== "object")
    throw new TypeError(`Swap at index ${index} must be an object.`);
  if (!swap.swapId)
    throw new TypeError(`Swap at index ${index}: swapId is required.`);
  if (!swap.expiresAt || isNaN(Date.parse(swap.expiresAt)))
    throw new TypeError(`Swap ${swap.swapId}: expiresAt must be a valid ISO date string.`);
  if (!swap.state)
    throw new TypeError(`Swap ${swap.swapId}: state is required.`);
}

function validateExtension(extension, swap) {
  if (typeof extension !== "number" || extension <= 0)
    throw new RangeError(`Swap ${swap.swapId}: extensionMs must be a positive number.`);
  if (extension < MIN_EXTENSION_MS)
    throw new RangeError(
      `Swap ${swap.swapId}: extensionMs must be at least ${MIN_EXTENSION_MS}ms (1 minute).`
    );
  if (extension > MAX_EXTENSION_MS)
    throw new RangeError(
      `Swap ${swap.swapId}: extensionMs must not exceed ${MAX_EXTENSION_MS}ms (30 days).`
    );
}

// ── Core logic ────────────────────────────────────────────────────────────────

function extendOne(swap, extensionMs, now) {
  if (!EXTENDABLE_STATES.has(swap.state)) {
    throw new Error(
      `Swap ${swap.swapId}: only PENDING or ACTIVE swaps can be extended (got '${swap.state}').`
    );
  }

  const currentExpiry = new Date(swap.expiresAt).getTime();
  const baseTime     = Math.max(currentExpiry, now);
  const newExpiryMs  = baseTime + extensionMs;
  const newExpiresAt = new Date(newExpiryMs).toISOString();

  return {
    swapId:          swap.swapId,
    state:           swap.state,
    previousExpiry:  swap.expiresAt,
    newExpiry:       newExpiresAt,
    extensionMs,
    extendedAt:      new Date(now).toISOString(),
  };
}

// ── Batch entry point ─────────────────────────────────────────────────────────

/**
 * Extend expiry for a batch of swaps.
 *
 * @param {Array<SwapEntry>} swaps
 * @param {Array<number>|number} extensions  - per-swap extensionMs OR single value applied to all
 * @param {{ now?: number }} [options]
 * @returns {BatchExpiryResult}
 *
 * @typedef {{ swapId: string, expiresAt: string, state: string }} SwapEntry
 *
 * @typedef {Object} BatchExpiryResult
 * @property {number} batchSize
 * @property {number} extendedCount
 * @property {number} failedCount
 * @property {number} totalExtensionMs
 * @property {Array}  results
 * @property {Array}  errors
 */
function extendBatchExpiry(swaps, extensions, options = {}) {
  if (!Array.isArray(swaps) || swaps.length === 0)
    throw new TypeError("swaps must be a non-empty array.");
  if (swaps.length > MAX_BATCH_SIZE)
    throw new RangeError(`Batch size ${swaps.length} exceeds maximum of ${MAX_BATCH_SIZE}.`);

  const now = options.now ?? Date.now();

  const extensionArr = typeof extensions === "number"
    ? Array(swaps.length).fill(extensions)
    : extensions;

  if (!Array.isArray(extensionArr) || extensionArr.length !== swaps.length)
    throw new TypeError("extensions must be a number or an array matching swaps length.");

  const results = [];
  const errors  = [];

  for (let i = 0; i < swaps.length; i++) {
    try {
      validateSwap(swaps[i], i);
      validateExtension(extensionArr[i], swaps[i]);
      results.push(extendOne(swaps[i], extensionArr[i], now));
    } catch (err) {
      errors.push({ index: i, swapId: swaps[i]?.swapId ?? null, error: err.message });
    }
  }

  const totalExtensionMs = results.reduce((s, r) => s + r.extensionMs, 0);

  return {
    batchSize:        swaps.length,
    extendedCount:    results.length,
    failedCount:      errors.length,
    totalExtensionMs,
    results,
    errors,
  };
}

module.exports = {
  extendBatchExpiry,
  EXTENDABLE_STATES,
  MAX_EXTENSION_MS,
  MIN_EXTENSION_MS,
  MAX_BATCH_SIZE,
};
