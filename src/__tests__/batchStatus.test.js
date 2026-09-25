const { BatchStatusTracker } = require("../batch/batchStatus");

describe("BatchStatusTracker", () => {
  test("tracks progress and exposes an aggregate completion state", () => {
    const tracker = new BatchStatusTracker();
    tracker.create("batch-1", ["op-1", "op-2"]);

    tracker.update("batch-1", "op-1", "RUNNING");
    let status = tracker.getStatus("batch-1");
    expect(status.state).toBe("RUNNING");
    expect(status.completed).toBe(0);

    status = tracker.update("batch-1", "op-1", "COMPLETED", { durationMs: 12 });
    expect(status.completed).toBe(1);
    expect(status.operations[0].durationMs).toBe(12);

    status = tracker.update("batch-1", "op-2", "COMPLETED");
    expect(status.state).toBe("COMPLETED");
    expect(status.completed).toBe(2);
  });

  test("marks a batch failed when any operation fails", () => {
    const tracker = new BatchStatusTracker();
    tracker.create("batch-2", ["op-1", "op-2"]);
    tracker.update("batch-2", "op-1", "FAILED", { error: "timeout" });
    tracker.update("batch-2", "op-2", "COMPLETED");

    expect(tracker.getStatus("batch-2")).toMatchObject({
      state: "FAILED",
      failed: 1,
      completed: 1,
    });
  });

  test("returns null for unknown batches and protects stored state", () => {
    const tracker = new BatchStatusTracker();
    expect(tracker.getStatus("missing")).toBeNull();
    const status = tracker.create("batch-3", ["op-1"]);
    status.operations[0].state = "FAILED";
    expect(tracker.getStatus("batch-3").operations[0].state).toBe("QUEUED");
  });
});
