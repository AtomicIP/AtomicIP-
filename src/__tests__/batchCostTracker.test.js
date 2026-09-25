const { calculateBatchCosts } = require("../batch/batchCostTracker");

describe("calculateBatchCosts", () => {
  test("reports per-operation and aggregate resource costs", () => {
    const result = calculateBatchCosts(
      [
        { id: "op-1", network: 2, compute: 10, storage: 1 },
        { id: "op-2", network: 1, compute: 5, storage: 0 },
      ],
      { network: 0.5, compute: 0.1, storage: 2 }
    );

    expect(result.costs[0]).toMatchObject({
      id: "op-1",
      breakdown: { network: 1, compute: 1, storage: 2 },
      total: 4,
    });
    expect(result.byResource).toEqual({ network: 1.5, compute: 1.5, storage: 2 });
    expect(result.total).toBe(5);
  });

  test("defaults omitted resource usage to zero", () => {
    const result = calculateBatchCosts([{ id: "op-1", compute: 4 }], { compute: 2 });
    expect(result.costs[0].usage).toEqual({ network: 0, compute: 4, storage: 0 });
    expect(result.total).toBe(8);
  });

  test("rejects invalid resource usage and rates", () => {
    expect(() => calculateBatchCosts([{ network: -1 }])).toThrow(RangeError);
    expect(() => calculateBatchCosts([{ network: 1 }], { network: NaN })).toThrow(RangeError);
  });
});
