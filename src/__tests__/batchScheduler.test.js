const {
  BatchScheduler,
  SCHEDULED,
  COMPLETED,
  CANCELLED,
} = require("../batch/batchScheduler");

describe("BatchScheduler", () => {
  const operation = {
    batchId: "batch-1",
    operations: [{ swapId: "swap-1" }],
  };

  test("defers execution until the operation is due", async () => {
    const scheduler = new BatchScheduler();
    const executor = jest.fn(async (items) => items.length);
    scheduler.schedule(operation, new Date("2026-01-01T00:00:10Z"), executor);

    await scheduler.runDue(new Date("2026-01-01T00:00:09Z"));
    expect(executor).not.toHaveBeenCalled();
    expect(scheduler.getStatus("batch-1").status).toBe(SCHEDULED);

    const results = await scheduler.runDue(new Date("2026-01-01T00:00:10Z"));
    expect(executor).toHaveBeenCalledWith(operation.operations);
    expect(results[0].status).toBe(COMPLETED);
    expect(results[0].result).toBe(1);
  });

  test("cancels a scheduled operation without executing it", async () => {
    const scheduler = new BatchScheduler();
    const executor = jest.fn();
    scheduler.schedule(operation, new Date("2026-01-01T00:00:10Z"), executor);

    expect(scheduler.cancel("batch-1").status).toBe(CANCELLED);
    await scheduler.runDue(new Date("2026-01-01T00:01:00Z"));
    expect(executor).not.toHaveBeenCalled();
  });

  test("records executor failures", async () => {
    const scheduler = new BatchScheduler();
    scheduler.schedule(
      operation,
      new Date("2026-01-01T00:00:00Z"),
      () => { throw new Error("worker unavailable"); }
    );

    const [result] = await scheduler.runDue(new Date("2026-01-01T00:00:01Z"));
    expect(result.status).toBe("FAILED");
    expect(result.error).toBe("worker unavailable");
  });
});
