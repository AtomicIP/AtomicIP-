/**
 * Bulk swap operations — Issue: batch efficiency improvements
 *
 * Allows a caller to submit a collection of related swap actions in a single
 * orchestration step rather than forcing one API round-trip per swap.
 */

function validateSwapOperation(operation, index) {
  if (!operation || typeof operation !== "object") {
    throw new TypeError(`Swap operation at index ${index} must be an object.`);
  }
  if (!operation.swapId && operation.swapId !== 0) {
    throw new TypeError(`Swap operation at index ${index} is missing swapId.`);
  }
  if (!operation.action) {
    throw new TypeError(`Swap operation at index ${index} is missing action.`);
  }
}

function bulkProcessSwaps(swaps, executor, options = {}) {
  if (!Array.isArray(swaps)) {
    throw new TypeError("swaps must be an array.");
  }
  if (typeof executor !== "function") {
    throw new TypeError("executor must be a function.");
  }

  const maxConcurrency = options.maxConcurrency ?? swaps.length || 1;
  const results = [];
  const errors = [];

  for (let index = 0; index < swaps.length; index += 1) {
    const swap = swaps[index];
    try {
      validateSwapOperation(swap, index);
      const result = executor(swap, index);
      results.push({ index, swapId: swap.swapId, status: "success", result });
    } catch (error) {
      errors.push({ index, swapId: swap?.swapId ?? null, status: "failed", error: error.message });
    }
  }

  return {
    batchSize: swaps.length,
    successCount: results.length,
    failedCount: errors.length,
    maxConcurrency,
    results,
    errors,
  };
}

module.exports = {
  bulkProcessSwaps,
  validateSwapOperation,
};
