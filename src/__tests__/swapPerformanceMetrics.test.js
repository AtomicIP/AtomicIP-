const {
  recordSwapEvent,
  calculateSwapPerformance,
  rankListingsByPerformance,
} = require("../metrics/swapPerformanceMetrics");

describe("swap performance metrics", () => {
  test("aggregates lifecycle outcomes and produces a pricing signal", () => {
    const events = [];
    recordSwapEvent(events, { swapId: "s1", status: "COMPLETED", value: 1000, durationMs: 50 });
    recordSwapEvent(events, { swapId: "s2", status: "CANCELLED", value: 500, durationMs: 20 });
    recordSwapEvent(events, { swapId: "s3", status: "COMPLETED", value: 1500, disputed: true, durationMs: 80 });

    const metrics = calculateSwapPerformance(events);

    expect(metrics.swapCount).toBe(3);
    expect(metrics.completedCount).toBe(2);
    expect(metrics.cancellationRate).toBeCloseTo(1 / 3);
    expect(metrics.disputeRate).toBeCloseTo(1 / 3);
    expect(metrics.recommendedPriceMultiplier).toBeGreaterThan(1);
    expect(metrics.discoveryScore).toBeLessThan(100);
  });

  test("uses the latest lifecycle event for each swap", () => {
    const events = [
      { swapId: "s1", status: "PENDING", recordedAt: "2026-01-01T00:00:00Z" },
      { swapId: "s1", status: "COMPLETED", value: 100, recordedAt: "2026-01-02T00:00:00Z" },
    ];

    expect(calculateSwapPerformance(events).completedCount).toBe(1);
    expect(calculateSwapPerformance(events).swapCount).toBe(1);
  });

  test("ranks listings by discovery score", () => {
    const ranked = rankListingsByPerformance([
      { id: "low", events: [{ swapId: "1", status: "CANCELLED" }] },
      { id: "high", events: [{ swapId: "2", status: "COMPLETED" }] },
    ]);
    expect(ranked.map((listing) => listing.id)).toEqual(["high", "low"]);
  });
});
