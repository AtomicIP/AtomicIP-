/**
 * Swap Batch Partial Payment — Issue #514
 * ─────────────────────────────────────────
 * Handles partial (installment) payments for a batch of swaps.
 *
 * Model:
 *  - Each swap has a total price and a paid-so-far amount.
 *  - A payment run allocates an available balance across swaps
 *    in priority order until the balance is exhausted or all
 *    swaps are fully paid.
 *  - Swaps that reach full payment are marked COMPLETED.
 *  - Swaps that receive a partial payment remain PENDING.
 */

const MAX_BATCH_SIZE = 100;

// ── Validation ────────────────────────────────────────────────────────────────

function validateSwapEntry(swap, index) {
  if (!swap || typeof swap !== "object")
    throw new TypeError(`Entry at index ${index} must be an object.`);
  if (!swap.swapId)
    throw new TypeError(`Entry at index ${index}: swapId is required.`);
  if (typeof swap.totalPrice !== "number" || swap.totalPrice <= 0)
    throw new RangeError(`Entry at index ${index}: totalPrice must be a positive number.`);
  if (typeof swap.paidAmount !== "number" || swap.paidAmount < 0)
    throw new RangeError(`Entry at index ${index}: paidAmount must be >= 0.`);
  if (swap.paidAmount > swap.totalPrice)
    throw new RangeError(`Entry at index ${index}: paidAmount (${swap.paidAmount}) exceeds totalPrice (${swap.totalPrice}).`);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/**
 * Calculate the remaining balance owed for a swap.
 *
 * @param {object} swap  - { totalPrice, paidAmount }
 * @returns {number}
 */
function remainingBalance(swap) {
  return swap.totalPrice - swap.paidAmount;
}

/**
 * Calculate the minimum installment amount for a swap.
 *
 * @param {object} swap
 * @param {number} installmentCount  - total number of installments
 * @returns {number}
 */
function installmentAmount(swap, installmentCount) {
  if (typeof installmentCount !== "number" || installmentCount < 1)
    throw new RangeError("installmentCount must be >= 1.");
  return Math.ceil(swap.totalPrice / installmentCount);
}

// ── Core ──────────────────────────────────────────────────────────────────────

/**
 * Apply a partial payment to a single swap.
 *
 * @param {object} swap     - { swapId, totalPrice, paidAmount }
 * @param {number} payment  - amount to apply
 * @returns {{ swapId, appliedAmount, newPaidAmount, remaining, status }}
 */
function applyPartialPayment(swap, payment) {
  if (typeof payment !== "number" || payment <= 0)
    throw new RangeError("payment must be a positive number.");

  const owed          = remainingBalance(swap);
  const appliedAmount = Math.min(payment, owed);
  const newPaidAmount = swap.paidAmount + appliedAmount;
  const remaining     = swap.totalPrice - newPaidAmount;
  const status        = remaining === 0 ? "COMPLETED" : "PENDING";

  return { swapId: swap.swapId, appliedAmount, newPaidAmount, remaining, status };
}

/**
 * Process partial payments for a batch of swaps from an available balance.
 *
 * Swaps are paid in the order provided. The balance is allocated greedily:
 * each swap receives as much as possible (up to its remaining balance)
 * until the available balance is exhausted.
 *
 * @param {Array<{ swapId, totalPrice, paidAmount }>} swaps
 * @param {number} availableBalance  - total funds available for this payment run
 * @param {object} [options]
 * @param {boolean} [options.allowPartial=true]  - if false, only fully-payable swaps are paid
 * @returns {BatchPartialPaymentResult}
 *
 * @typedef {Object} SwapPaymentOutcome
 * @property {string|number} swapId
 * @property {number}        appliedAmount
 * @property {number}        newPaidAmount
 * @property {number}        remaining
 * @property {string}        status   - "COMPLETED" | "PENDING" | "SKIPPED"
 *
 * @typedef {Object} BatchPartialPaymentResult
 * @property {number}               batchSize
 * @property {number}               availableBalance
 * @property {number}               totalApplied
 * @property {number}               remainingBalance
 * @property {number}               completedCount
 * @property {number}               partialCount
 * @property {number}               skippedCount
 * @property {SwapPaymentOutcome[]} outcomes
 */
function processBatchPartialPayments(swaps, availableBalance, options = {}) {
  if (!Array.isArray(swaps) || swaps.length === 0)
    throw new TypeError("swaps must be a non-empty array.");
  if (swaps.length > MAX_BATCH_SIZE)
    throw new RangeError(`Batch size ${swaps.length} exceeds maximum of ${MAX_BATCH_SIZE}.`);
  if (typeof availableBalance !== "number" || availableBalance < 0)
    throw new RangeError("availableBalance must be a non-negative number.");

  swaps.forEach(validateSwapEntry);

  const { allowPartial = true } = options;

  let balance  = availableBalance;
  const outcomes = [];

  for (const swap of swaps) {
    const owed = remainingBalance(swap);

    if (owed === 0) {
      // Already fully paid
      outcomes.push({
        swapId:        swap.swapId,
        appliedAmount: 0,
        newPaidAmount: swap.paidAmount,
        remaining:     0,
        status:        "COMPLETED",
      });
      continue;
    }

    if (balance <= 0) {
      outcomes.push({
        swapId:        swap.swapId,
        appliedAmount: 0,
        newPaidAmount: swap.paidAmount,
        remaining:     owed,
        status:        "SKIPPED",
      });
      continue;
    }

    if (!allowPartial && balance < owed) {
      // Strict mode: skip if we can't fully pay this swap
      outcomes.push({
        swapId:        swap.swapId,
        appliedAmount: 0,
        newPaidAmount: swap.paidAmount,
        remaining:     owed,
        status:        "SKIPPED",
      });
      continue;
    }

    const result = applyPartialPayment(swap, balance);
    balance -= result.appliedAmount;
    outcomes.push(result);
  }

  const totalApplied    = outcomes.reduce((s, o) => s + o.appliedAmount, 0);
  const completedCount  = outcomes.filter((o) => o.status === "COMPLETED").length;
  const partialCount    = outcomes.filter((o) => o.status === "PENDING").length;
  const skippedCount    = outcomes.filter((o) => o.status === "SKIPPED").length;

  return {
    batchSize:        swaps.length,
    availableBalance,
    totalApplied,
    remainingBalance: availableBalance - totalApplied,
    completedCount,
    partialCount,
    skippedCount,
    outcomes,
  };
}

/**
 * Calculate how many installments remain for each swap in a batch.
 *
 * @param {Array<{ swapId, totalPrice, paidAmount }>} swaps
 * @param {number} installmentSize  - fixed installment amount
 * @returns {Array<{ swapId, installmentsRemaining, nextPaymentAmount }>}
 */
function calculateRemainingInstallments(swaps, installmentSize) {
  if (!Array.isArray(swaps)) throw new TypeError("swaps must be an array.");
  if (typeof installmentSize !== "number" || installmentSize <= 0)
    throw new RangeError("installmentSize must be a positive number.");

  return swaps.map((swap, i) => {
    validateSwapEntry(swap, i);
    const owed                  = remainingBalance(swap);
    const installmentsRemaining = Math.ceil(owed / installmentSize);
    const nextPaymentAmount     = Math.min(installmentSize, owed);
    return { swapId: swap.swapId, installmentsRemaining, nextPaymentAmount, owed };
  });
}

/**
 * Summarise payment progress for a batch.
 *
 * @param {Array<{ swapId, totalPrice, paidAmount }>} swaps
 * @returns {{ batchSize, totalValue, totalPaid, totalOwed, completionPct, swapSummaries }}
 */
function batchPaymentSummary(swaps) {
  if (!Array.isArray(swaps)) throw new TypeError("swaps must be an array.");
  swaps.forEach(validateSwapEntry);

  const totalValue = swaps.reduce((s, sw) => s + sw.totalPrice, 0);
  const totalPaid  = swaps.reduce((s, sw) => s + sw.paidAmount, 0);
  const totalOwed  = totalValue - totalPaid;

  const swapSummaries = swaps.map((sw) => ({
    swapId:        sw.swapId,
    totalPrice:    sw.totalPrice,
    paidAmount:    sw.paidAmount,
    owed:          remainingBalance(sw),
    completionPct: totalValue > 0 ? +((sw.paidAmount / sw.totalPrice) * 100).toFixed(2) : 0,
    status:        remainingBalance(sw) === 0 ? "COMPLETED" : "PENDING",
  }));

  return {
    batchSize:     swaps.length,
    totalValue,
    totalPaid,
    totalOwed,
    completionPct: totalValue > 0 ? +((totalPaid / totalValue) * 100).toFixed(2) : 0,
    swapSummaries,
  };
}

module.exports = {
  processBatchPartialPayments,
  applyPartialPayment,
  calculateRemainingInstallments,
  batchPaymentSummary,
  remainingBalance,
  installmentAmount,
  MAX_BATCH_SIZE,
};
