/**
 * Swap Batch Conditional Completion — Issue #513
 * ─────────────────────────────────────────────────
 * Evaluates completion conditions for a batch of swaps and
 * completes only those that satisfy all their conditions.
 *
 * Supported condition types:
 *  - KEY_VALID      : decryption key hash matches expected hash
 *  - PRICE_BELOW    : swap price is below a threshold
 *  - TIME_AFTER     : current timestamp is after a given epoch (ms)
 *  - CUSTOM         : caller-supplied predicate function
 */

const MAX_BATCH_SIZE = 100;

// ── Condition types ───────────────────────────────────────────────────────────

const ConditionType = Object.freeze({
  KEY_VALID:   "KEY_VALID",
  PRICE_BELOW: "PRICE_BELOW",
  TIME_AFTER:  "TIME_AFTER",
  CUSTOM:      "CUSTOM",
});

// ── Validation ────────────────────────────────────────────────────────────────

function validateSwapEntry(swap, index) {
  if (!swap || typeof swap !== "object")
    throw new TypeError(`Entry at index ${index} must be an object.`);
  if (!swap.swapId)
    throw new TypeError(`Entry at index ${index}: swapId is required.`);
  if (typeof swap.price !== "number" || swap.price <= 0)
    throw new RangeError(`Entry at index ${index}: price must be a positive number.`);
  if (!Array.isArray(swap.conditions))
    throw new TypeError(`Entry at index ${index}: conditions must be an array.`);
}

function validateCondition(cond, swapId) {
  if (!cond || typeof cond !== "object")
    throw new TypeError(`Swap ${swapId}: condition must be an object.`);
  if (!Object.values(ConditionType).includes(cond.type))
    throw new TypeError(`Swap ${swapId}: unknown condition type "${cond.type}".`);
}

// ── Condition evaluators ──────────────────────────────────────────────────────

/**
 * Evaluate a single condition against a swap and context.
 *
 * @param {object} condition  - { type, threshold?, expectedKeyHash?, predicate? }
 * @param {object} swap       - { swapId, price, keyHash?, ... }
 * @param {object} ctx        - { nowMs?: number }
 * @returns {{ passed: boolean, reason: string }}
 */
function evaluateCondition(condition, swap, ctx = {}) {
  const nowMs = ctx.nowMs ?? Date.now();

  switch (condition.type) {
    case ConditionType.KEY_VALID: {
      const passed = typeof swap.keyHash === "string" &&
                     swap.keyHash === condition.expectedKeyHash;
      return {
        passed,
        reason: passed ? "key valid" : "key hash mismatch or missing",
      };
    }

    case ConditionType.PRICE_BELOW: {
      if (typeof condition.threshold !== "number")
        throw new TypeError(`Swap ${swap.swapId}: PRICE_BELOW requires a numeric threshold.`);
      const passed = swap.price < condition.threshold;
      return {
        passed,
        reason: passed
          ? `price ${swap.price} < ${condition.threshold}`
          : `price ${swap.price} >= ${condition.threshold}`,
      };
    }

    case ConditionType.TIME_AFTER: {
      if (typeof condition.afterMs !== "number")
        throw new TypeError(`Swap ${swap.swapId}: TIME_AFTER requires a numeric afterMs.`);
      const passed = nowMs >= condition.afterMs;
      return {
        passed,
        reason: passed
          ? `now (${nowMs}) >= ${condition.afterMs}`
          : `now (${nowMs}) < ${condition.afterMs}`,
      };
    }

    case ConditionType.CUSTOM: {
      if (typeof condition.predicate !== "function")
        throw new TypeError(`Swap ${swap.swapId}: CUSTOM condition requires a predicate function.`);
      const passed = Boolean(condition.predicate(swap, ctx));
      return { passed, reason: passed ? "custom predicate passed" : "custom predicate failed" };
    }

    default:
      return { passed: false, reason: `unknown condition type: ${condition.type}` };
  }
}

/**
 * Evaluate all conditions for a single swap.
 *
 * @param {object}   swap
 * @param {object}   ctx
 * @returns {{ eligible: boolean, conditionResults: object[] }}
 */
function evaluateSwapConditions(swap, ctx = {}) {
  if (swap.conditions.length === 0) {
    return { eligible: true, conditionResults: [] };
  }

  const conditionResults = swap.conditions.map((cond) => {
    validateCondition(cond, swap.swapId);
    const result = evaluateCondition(cond, swap, ctx);
    return { type: cond.type, ...result };
  });

  const eligible = conditionResults.every((r) => r.passed);
  return { eligible, conditionResults };
}

// ── Core ──────────────────────────────────────────────────────────────────────

/**
 * Process conditional completion for a batch of swaps.
 * Swaps that satisfy all conditions are marked completed;
 * those that fail are left pending with a reason.
 *
 * @param {Array<{ swapId, price, conditions, keyHash? }>} swaps
 * @param {object} [ctx]  - { nowMs?: number }
 * @returns {BatchConditionalResult}
 *
 * @typedef {Object} BatchConditionalResult
 * @property {number}   batchSize
 * @property {number}   completed
 * @property {number}   skipped
 * @property {number}   failed
 * @property {object[]} results    - per-swap outcome
 * @property {object[]} errors
 */
function processBatchConditionalCompletion(swaps, ctx = {}) {
  if (!Array.isArray(swaps) || swaps.length === 0)
    throw new TypeError("swaps must be a non-empty array.");
  if (swaps.length > MAX_BATCH_SIZE)
    throw new RangeError(`Batch size ${swaps.length} exceeds maximum of ${MAX_BATCH_SIZE}.`);

  swaps.forEach(validateSwapEntry);

  const results = [];
  const errors  = [];

  for (const swap of swaps) {
    try {
      const { eligible, conditionResults } = evaluateSwapConditions(swap, ctx);
      results.push({
        swapId:           swap.swapId,
        eligible,
        status:           eligible ? "COMPLETED" : "SKIPPED",
        conditionResults,
        completedAt:      eligible ? (ctx.nowMs ?? Date.now()) : null,
      });
    } catch (err) {
      errors.push({ swapId: swap.swapId, error: err.message });
    }
  }

  const completed = results.filter((r) => r.status === "COMPLETED").length;
  const skipped   = results.filter((r) => r.status === "SKIPPED").length;

  return {
    batchSize: swaps.length,
    completed,
    skipped,
    failed:    errors.length,
    results,
    errors,
  };
}

/**
 * Filter a batch to only swaps that are eligible for completion.
 *
 * @param {object[]} swaps
 * @param {object}   [ctx]
 * @returns {object[]} eligible swaps
 */
function filterEligibleSwaps(swaps, ctx = {}) {
  if (!Array.isArray(swaps)) throw new TypeError("swaps must be an array.");
  return swaps.filter((swap) => {
    try {
      const { eligible } = evaluateSwapConditions(swap, ctx);
      return eligible;
    } catch {
      return false;
    }
  });
}

/**
 * Check whether a single swap meets all its conditions.
 *
 * @param {object} swap
 * @param {object} [ctx]
 * @returns {boolean}
 */
function isSwapEligible(swap, ctx = {}) {
  try {
    return evaluateSwapConditions(swap, ctx).eligible;
  } catch {
    return false;
  }
}

module.exports = {
  processBatchConditionalCompletion,
  evaluateSwapConditions,
  evaluateCondition,
  filterEligibleSwaps,
  isSwapEligible,
  ConditionType,
  MAX_BATCH_SIZE,
};
