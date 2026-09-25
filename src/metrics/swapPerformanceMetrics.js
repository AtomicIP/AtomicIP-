/**
 * Swap Performance Metrics
 *
 * Aggregates swap lifecycle events into metrics suitable for pricing and
 * discovery. Events are plain objects so callers can persist them in their
 * existing event store without coupling this module to a database.
 */

const TERMINAL_OUTCOMES = new Set(['COMPLETED', 'CANCELLED', 'FAILED']);

function validateEvent(event, index = 0) {
  if (!event || typeof event !== 'object')
  {throw new TypeError(`Swap event at index ${index} must be an object.`);}
  if (!event.swapId)
  {throw new TypeError(`Swap event at index ${index}: swapId is required.`);}
  if (!event.status || typeof event.status !== 'string')
  {throw new TypeError(`Swap event ${event.swapId}: status is required.`);}
  if (event.value !== null && event.value !== undefined &&
      (typeof event.value !== 'number' || event.value < 0))
  {throw new RangeError(`Swap event ${event.swapId}: value must be non-negative.`);}
  if (event.durationMs !== null && event.durationMs !== undefined &&
      (typeof event.durationMs !== 'number' || event.durationMs < 0))
  {throw new RangeError(`Swap event ${event.swapId}: durationMs must be non-negative.`);}
}

function recordSwapEvent(events, event) {
  if (!Array.isArray(events)) {throw new TypeError('events must be an array.');}
  validateEvent(event);
  const recorded = {
    ...event,
    status: event.status.toUpperCase(),
    recordedAt: event.recordedAt ?? new Date().toISOString()
  };
  events.push(recorded);
  return recorded;
}

/**
 * Aggregate one event per swap. If a lifecycle log contains multiple events
 * for a swap, the latest event by recordedAt is used.
 */
function calculateSwapPerformance(events, options = {}) {
  if (!Array.isArray(events)) {throw new TypeError('events must be an array.');}
  events.forEach(validateEvent);

  const latestBySwap = new Map();
  for (const event of events) {
    const current = latestBySwap.get(event.swapId);
    if (!current || new Date(event.recordedAt ?? 0) >= new Date(current.recordedAt ?? 0)) {
      latestBySwap.set(event.swapId, event);
    }
  }

  const swaps = [...latestBySwap.values()];
  const totalValue = swaps.reduce((sum, event) => sum + (event.value ?? 0), 0);
  const completed = swaps.filter((event) => event.status.toUpperCase() === 'COMPLETED');
  const cancelled = swaps.filter((event) => event.status.toUpperCase() === 'CANCELLED');
  const failed = swaps.filter((event) => event.status.toUpperCase() === 'FAILED');
  const disputed = swaps.filter((event) => event.disputed === true);
  const denominator = swaps.length || 1;
  const completionRate = completed.length / denominator;
  const disputeRate = disputed.length / denominator;
  const cancellationRate = cancelled.length / denominator;
  const averageDurationMs = swaps.reduce((sum, event) => sum + (event.durationMs ?? 0), 0) / denominator;
  const averageValue = totalValue / denominator;

  const targetCompletionRate = options.targetCompletionRate ?? 0.8;
  const riskPremium = Math.min(0.2, disputeRate * 0.5 + cancellationRate * 0.25);
  const demandDiscount = Math.min(0.1, Math.max(0, targetCompletionRate - completionRate) * 0.2);
  const recommendedPriceMultiplier = +(1 + riskPremium - demandDiscount).toFixed(4);

  return {
    swapCount: swaps.length,
    totalValue,
    averageValue,
    completedCount: completed.length,
    cancelledCount: cancelled.length,
    failedCount: failed.length,
    disputedCount: disputed.length,
    completionRate,
    cancellationRate,
    disputeRate,
    averageDurationMs,
    recommendedPriceMultiplier,
    discoveryScore: +(completionRate * (1 - disputeRate) * 100).toFixed(2)
  };
}

function rankListingsByPerformance(listings, options = {}) {
  if (!Array.isArray(listings)) {throw new TypeError('listings must be an array.');}
  return listings
    .map((listing, index) => {
      if (!listing || typeof listing !== 'object' || !listing.id)
      {throw new TypeError(`Listing at index ${index}: id is required.`);}
      const metrics = calculateSwapPerformance(listing.events ?? [], options);
      return { ...listing, metrics, discoveryScore: metrics.discoveryScore };
    })
    .sort((a, b) => b.discoveryScore - a.discoveryScore);
}

module.exports = {
  validateEvent,
  recordSwapEvent,
  calculateSwapPerformance,
  rankListingsByPerformance,
  TERMINAL_OUTCOMES
};
