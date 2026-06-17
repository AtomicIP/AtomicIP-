/**
 * Swap Batch Cancellation
 * ───────────────────────
 * Cancel multiple swaps in one operation.
 *
 * Cancellation rules:
 *  - Only PENDING or ACTIVE swaps may be cancelled
 *  - Each swap may specify a cancellation reason
 *  - Partial batches succeed: non-cancellable swaps are logged as errors
 *  - Refund eligibility is determined per cancellation policy
 */

const CANCELLABLE_STATES = new Set(["PENDING", "ACTIVE"]);
const CANCELLED_STATE    = "CANCELLED";
const MAX_BATCH_SIZE     = 100;
const MAX_REASON_LENGTH  = 256;

const REFUND_POLICIES = Object.freeze({
  FULL:    "FULL",    // full refund to initiator
  PARTIAL: "PARTIAL", // partial refund (e.g. minus fees already incurred)
  NONE:    "NONE",    // no refund (e.g. penalty cancellation)
});

// ── Validators ────────────────────────────────────────────────────────────────

function validateSwap(swap, index) {
  if (!swap || typeof swap !== "object")
    throw new TypeError(`Swap at index ${index} must be an object.`);
  if (!swap.swapId)
    throw new TypeError(`Swap at index ${index}: swapId is required.`);
  if (!swap.state)
    throw new TypeError(`Swap ${swap.swapId}: state is required.`);
  if (typeof swap.amount !== "number" || swap.amount <= 0)
    throw new RangeError(`Swap ${swap.swapId}: amount must be a positive number.`);
}

function validateCancellation(cancellation, swap) {
  if (cancellation && typeof cancellation !== "object")
    throw new TypeError(`Cancellation for swap ${swap.swapId} must be an object or null.`);
  const reason = cancellation?.reason ?? "";
  if (typeof reason !== "string")
    throw new TypeError(`Swap ${swap.swapId}: reason must be a string.`);
  if (reason.length > MAX_REASON_LENGTH)
    throw new RangeError(
      `Swap ${swap.swapId}: reason must not exceed ${MAX_REASON_LENGTH} characters.`
    );
  const policy = cancellation?.refundPolicy;
  if (policy && !Object.values(REFUND_POLICIES).includes(policy))
    throw new TypeError(
      `Swap ${swap.swapId}: invalid refundPolicy '${policy}'.`
    );
}

// ── Refund calculation ────────────────────────────────────────────────────────

function calculateRefund(amount, policy = REFUND_POLICIES.FULL, feePaid = 0) {
  switch (policy) {
    case REFUND_POLICIES.FULL:
      return amount;
    case REFUND_POLICIES.PARTIAL:
      return Math.max(0, +(amount - feePaid).toFixed(8));
    case REFUND_POLICIES.NONE:
      return 0;
    default:
      return amount;
  }
}

// ── Core cancellation ─────────────────────────────────────────────────────────

function cancelOne(swap, cancellation, now) {
  if (!CANCELLABLE_STATES.has(swap.state)) {
    throw new Error(
      `Swap ${swap.swapId}: cannot cancel a swap in state '${swap.state}'.`
    );
  }

  const policy    = cancellation?.refundPolicy ?? REFUND_POLICIES.FULL;
  const feePaid   = cancellation?.feePaid ?? 0;
  const reason    = cancellation?.reason ?? "";
  const refund    = calculateRefund(swap.amount, policy, feePaid);

  return {
    swapId:        swap.swapId,
    previousState: swap.state,
    newState:      CANCELLED_STATE,
    amount:        swap.amount,
    refundAmount:  refund,
    refundPolicy:  policy,
    reason,
    cancelledAt:   new Date(now).toISOString(),
  };
}

// ── Batch entry point ─────────────────────────────────────────────────────────

/**
 * Cancel a batch of swaps.
 *
 * @param {Array<SwapEntry>} swaps
 * @param {Array<CancelEntry>|null} cancellations  - parallel array or null (default policy for all)
 * @param {{ now?: number }} [options]
 * @returns {BatchCancelResult}
 *
 * @typedef {{ swapId: string, state: string, amount: number }} SwapEntry
 * @typedef {{ reason?: string, refundPolicy?: string, feePaid?: number }} CancelEntry
 *
 * @typedef {Object} BatchCancelResult
 * @property {number} batchSize
 * @property {number} cancelledCount
 * @property {number} failedCount
 * @property {number} totalRefunded
 * @property {number} totalAmount
 * @property {Array}  results
 * @property {Array}  errors
 */
function cancelBatchSwaps(swaps, cancellations = null, options = {}) {
  if (!Array.isArray(swaps) || swaps.length === 0)
    throw new TypeError("swaps must be a non-empty array.");
  if (swaps.length > MAX_BATCH_SIZE)
    throw new RangeError(`Batch size ${swaps.length} exceeds maximum of ${MAX_BATCH_SIZE}.`);

  const now = options.now ?? Date.now();

  const cancelArr = cancellations === null
    ? Array(swaps.length).fill(null)
    : cancellations;

  if (!Array.isArray(cancelArr) || cancelArr.length !== swaps.length)
    throw new TypeError("cancellations must be null or an array matching swaps length.");

  const results = [];
  const errors  = [];

  for (let i = 0; i < swaps.length; i++) {
    try {
      validateSwap(swaps[i], i);
      validateCancellation(cancelArr[i], swaps[i]);
      results.push(cancelOne(swaps[i], cancelArr[i], now));
    } catch (err) {
      errors.push({ index: i, swapId: swaps[i]?.swapId ?? null, error: err.message });
    }
  }

  const totalRefunded = results.reduce((s, r) => s + r.refundAmount, 0);
  const totalAmount   = results.reduce((s, r) => s + r.amount, 0);

  return {
    batchSize:       swaps.length,
    cancelledCount:  results.length,
    failedCount:     errors.length,
    totalRefunded:   +totalRefunded.toFixed(8),
    totalAmount:     +totalAmount.toFixed(8),
    results,
    errors,
  };
}

module.exports = {
  cancelBatchSwaps,
  CANCELLABLE_STATES,
  REFUND_POLICIES,
  MAX_BATCH_SIZE,
  CANCELLED_STATE,
};
