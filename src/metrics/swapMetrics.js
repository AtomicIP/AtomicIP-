/**
 * Swap performance metrics — Issue: pricing optimization
 *
 * Provides a lightweight metrics surface for swap throughput, latency,
 * success rate, and realized pricing efficiency.
 */

function summarizeSwapMetrics(events = []) {
  if (!Array.isArray(events)) {
    throw new TypeError("events must be an array.");
  }

  const metrics = {
    totalSwaps: events.length,
    successfulSwaps: 0,
    failedSwaps: 0,
    averageLatencyMs: 0,
    averagePrice: 0,
    totalValue: 0,
  };

  if (events.length === 0) {
    return metrics;
  }

  let latencyTotal = 0;
  let priceTotal = 0;

  for (const event of events) {
    if (!event || typeof event !== "object") continue;
    if (event.success === true) metrics.successfulSwaps += 1;
    if (event.success === false) metrics.failedSwaps += 1;
    if (typeof event.latencyMs === "number" && Number.isFinite(event.latencyMs)) {
      latencyTotal += event.latencyMs;
    }
    if (typeof event.price === "number" && Number.isFinite(event.price)) {
      priceTotal += event.price;
      metrics.totalValue += event.price;
    }
  }

  metrics.averageLatencyMs = Math.round(latencyTotal / Math.max(events.length, 1));
  metrics.averagePrice = priceTotal > 0 ? Math.round(priceTotal / Math.max(metrics.successfulSwaps || 1, 1)) : 0;

  return metrics;
}

function buildSwapPerformanceReport(events = []) {
  const summary = summarizeSwapMetrics(events);
  const successRate = summary.totalSwaps === 0 ? 0 : (summary.successfulSwaps / summary.totalSwaps) * 100;

  return {
    ...summary,
    successRate,
    reportAt: new Date().toISOString(),
  };
}

module.exports = {
  summarizeSwapMetrics,
  buildSwapPerformanceReport,
};
