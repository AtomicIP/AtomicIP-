const {
  summarizeSwapMetrics,
  buildSwapPerformanceReport,
} = require('../metrics/swapMetrics');

describe('summarizeSwapMetrics', () => {
  test('computes metrics from fixed event array', () => {
    const events = [
      { success: true, latencyMs: 100, price: 10, value: 1000 },
      { success: true, latencyMs: 200, price: 20, value: 2000 },
      { success: false, latencyMs: 300, price: 30, value: 3000 },
      { success: true, latencyMs: 400, price: 40, value: 4000 },
    ];

    const result = summarizeSwapMetrics(events);

    expect(result.successCount).toBe(3);
    expect(result.failureCount).toBe(1);
    expect(result.totalCount).toBe(4);
    expect(result.averageLatencyMs).toBe(250);
    expect(result.averagePrice).toBe(25);
    expect(result.totalValue).toBe(10000);
    expect(result.successRate).toBe(0.75);
  });

  test('computes metrics for all-success events', () => {
    const events = [
      { success: true, latencyMs: 50, price: 5, value: 500 },
      { success: true, latencyMs: 150, price: 15, value: 1500 },
    ];

    const result = summarizeSwapMetrics(events);

    expect(result.successCount).toBe(2);
    expect(result.failureCount).toBe(0);
    expect(result.totalCount).toBe(2);
    expect(result.averageLatencyMs).toBe(100);
    expect(result.averagePrice).toBe(10);
    expect(result.totalValue).toBe(2000);
    expect(result.successRate).toBe(1);
  });

  test('computes metrics for all-failure events', () => {
    const events = [
      { success: false, latencyMs: 10, price: 1, value: 100 },
      { success: false, latencyMs: 30, price: 3, value: 300 },
    ];

    const result = summarizeSwapMetrics(events);

    expect(result.successCount).toBe(0);
    expect(result.failureCount).toBe(2);
    expect(result.totalCount).toBe(2);
    expect(result.averageLatencyMs).toBe(20);
    expect(result.averagePrice).toBe(2);
    expect(result.totalValue).toBe(400);
    expect(result.successRate).toBe(0);
  });

  test('returns zeroed metrics for empty input array', () => {
    const result = summarizeSwapMetrics([]);

    expect(result.successCount).toBe(0);
    expect(result.failureCount).toBe(0);
    expect(result.totalCount).toBe(0);
    expect(result.averageLatencyMs).toBe(0);
    expect(result.averagePrice).toBe(0);
    expect(result.totalValue).toBe(0);
    expect(result.successRate).toBe(0);
  });

  test('throws TypeError for non-array input', () => {
    expect(() => summarizeSwapMetrics(null)).toThrow(TypeError);
    expect(() => summarizeSwapMetrics(undefined)).toThrow(TypeError);
    expect(() => summarizeSwapMetrics({})).toThrow(TypeError);
    expect(() => summarizeSwapMetrics('not-an-array')).toThrow(TypeError);
    expect(() => summarizeSwapMetrics(42)).toThrow(TypeError);
  });
});

describe('buildSwapPerformanceReport', () => {
  test('includes successRate and reportAt', () => {
    const events = [
      { success: true, latencyMs: 100, price: 10, value: 1000 },
      { success: false, latencyMs: 300, price: 30, value: 3000 },
    ];

    const report = buildSwapPerformanceReport(events);

    expect(report.successRate).toBe(0.5);
    expect(report).toHaveProperty('reportAt');
    expect(report.reportAt).toBeDefined();
  });

  test('includes computed metrics from events', () => {
    const events = [
      { success: true, latencyMs: 100, price: 10, value: 1000 },
      { success: true, latencyMs: 200, price: 20, value: 2000 },
      { success: false, latencyMs: 300, price: 30, value: 3000 },
      { success: true, latencyMs: 400, price: 40, value: 4000 },
    ];

    const report = buildSwapPerformanceReport(events);

    expect(report.successCount).toBe(3);
    expect(report.failureCount).toBe(1);
    expect(report.totalCount).toBe(4);
    expect(report.averageLatencyMs).toBe(250);
    expect(report.averagePrice).toBe(25);
    expect(report.totalValue).toBe(10000);
    expect(report.successRate).toBe(0.75);
  });

  test('handles empty input array', () => {
    const report = buildSwapPerformanceReport([]);

    expect(report.successCount).toBe(0);
    expect(report.failureCount).toBe(0);
    expect(report.totalCount).toBe(0);
    expect(report.averageLatencyMs).toBe(0);
    expect(report.averagePrice).toBe(0);
    expect(report.totalValue).toBe(0);
    expect(report.successRate).toBe(0);
    expect(report).toHaveProperty('reportAt');
  });

  test('throws TypeError for non-array input', () => {
    expect(() => buildSwapPerformanceReport(null)).toThrow(TypeError);
    expect(() => buildSwapPerformanceReport(undefined)).toThrow(TypeError);
    expect(() => buildSwapPerformanceReport({})).toThrow(TypeError);
  });
});