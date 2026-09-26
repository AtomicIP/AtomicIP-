/**
 * Batch operation cost tracking.
 *
 * Costs are calculated from caller-provided resource usage and rates so the
 * accounting model can match the execution environment without hidden values.
 */

const MAX_BATCH_SIZE = 100;
const RESOURCE_TYPES = ['network', 'compute', 'storage'];

function validateNumber(value, name) {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0)
  {throw new RangeError(`${name} must be a finite non-negative number.`);}
}

function calculateBatchCosts(operations, rates = {}) {
  if (!Array.isArray(operations) || operations.length === 0)
  {throw new TypeError('operations must be a non-empty array.');}
  if (operations.length > MAX_BATCH_SIZE)
  {throw new RangeError(`Batch size ${operations.length} exceeds maximum of ${MAX_BATCH_SIZE}.`);}

  const unitRates = Object.fromEntries(
    RESOURCE_TYPES.map((type) => {
      const rate = rates[type] ?? 0;
      validateNumber(rate, `rates.${type}`);
      return [type, rate];
    })
  );

  const costs = operations.map((operation, index) => {
    if (!operation || typeof operation !== 'object')
    {throw new TypeError(`Operation at index ${index} must be an object.`);}
    const usage = Object.fromEntries(
      RESOURCE_TYPES.map((type) => {
        const value = operation[type] ?? 0;
        validateNumber(value, `operation ${operation.id ?? index}.${type}`);
        return [type, value];
      })
    );
    const breakdown = Object.fromEntries(
      RESOURCE_TYPES.map((type) => [type, +(usage[type] * unitRates[type]).toFixed(8)])
    );
    const total = RESOURCE_TYPES.reduce((sum, type) => sum + breakdown[type], 0);
    return {
      id: operation.id ?? index,
      usage,
      breakdown,
      total: +total.toFixed(8)
    };
  });

  const byResource = Object.fromEntries(
    RESOURCE_TYPES.map((type) => [
      type,
      +costs.reduce((sum, cost) => sum + cost.breakdown[type], 0).toFixed(8)
    ])
  );
  return {
    batchSize: operations.length,
    rates: unitRates,
    byResource,
    total: +Object.values(byResource).reduce((sum, value) => sum + value, 0).toFixed(8),
    costs
  };
}

module.exports = { calculateBatchCosts, RESOURCE_TYPES, MAX_BATCH_SIZE };
