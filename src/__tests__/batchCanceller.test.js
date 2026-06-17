const {
  cancelBatchSwaps,
  REFUND_POLICIES,
  MAX_BATCH_SIZE,
  CANCELLED_STATE,
} = require("../batch/batchCanceller");

const pendingSwap = (id, amount = 1000) => ({ swapId: id, state: "PENDING", amount });
const activeSwap  = (id, amount = 500)  => ({ swapId: id, state: "ACTIVE",  amount });

describe("cancelBatchSwaps — validation", () => {
  test("throws on empty swaps array", () => {
    expect(() => cancelBatchSwaps([])).toThrow(TypeError);
  });
  test("throws on batch > MAX_BATCH_SIZE", () => {
    const big = Array.from({ length: MAX_BATCH_SIZE + 1 }, (_, i) => pendingSwap(`s${i}`));
    expect(() => cancelBatchSwaps(big)).toThrow(RangeError);
  });
  test("throws on mismatched cancellations length", () => {
    expect(() => cancelBatchSwaps([pendingSwap("a")], [])).toThrow(TypeError);
  });
  test("records error for non-cancellable state", () => {
    const completed = { swapId: "c1", state: "COMPLETED", amount: 100 };
    const result = cancelBatchSwaps([completed]);
    expect(result.failedCount).toBe(1);
    expect(result.errors[0].swapId).toBe("c1");
  });
  test("records error for reason exceeding max length", () => {
    const longReason = "x".repeat(257);
    const result = cancelBatchSwaps([pendingSwap("r1")], [{ reason: longReason }]);
    expect(result.failedCount).toBe(1);
  });
  test("records error for invalid refundPolicy", () => {
    const result = cancelBatchSwaps([pendingSwap("p1")], [{ refundPolicy: "INVALID" }]);
    expect(result.failedCount).toBe(1);
  });
});

describe("cancelBatchSwaps — refund policies", () => {
  test("FULL policy refunds full amount", () => {
    const result = cancelBatchSwaps(
      [pendingSwap("f1", 1000)],
      [{ refundPolicy: REFUND_POLICIES.FULL }]
    );
    expect(result.results[0].refundAmount).toBe(1000);
    expect(result.totalRefunded).toBe(1000);
  });

  test("PARTIAL policy deducts feePaid", () => {
    const result = cancelBatchSwaps(
      [pendingSwap("f2", 1000)],
      [{ refundPolicy: REFUND_POLICIES.PARTIAL, feePaid: 50 }]
    );
    expect(result.results[0].refundAmount).toBeCloseTo(950);
  });

  test("NONE policy refunds 0", () => {
    const result = cancelBatchSwaps(
      [pendingSwap("f3", 1000)],
      [{ refundPolicy: REFUND_POLICIES.NONE }]
    );
    expect(result.results[0].refundAmount).toBe(0);
    expect(result.totalRefunded).toBe(0);
  });

  test("default policy (null cancellations) gives full refund", () => {
    const result = cancelBatchSwaps([pendingSwap("f4", 500)]);
    expect(result.results[0].refundAmount).toBe(500);
  });
});

describe("cancelBatchSwaps — state and counts", () => {
  test("sets newState to CANCELLED", () => {
    const result = cancelBatchSwaps([pendingSwap("s1")]);
    expect(result.results[0].newState).toBe(CANCELLED_STATE);
  });

  test("ACTIVE swap is cancellable", () => {
    const result = cancelBatchSwaps([activeSwap("a1")]);
    expect(result.cancelledCount).toBe(1);
  });

  test("mixed batch: valid and invalid counted correctly", () => {
    const swaps = [pendingSwap("m1"), { swapId: "m2", state: "EXPIRED", amount: 100 }];
    const result = cancelBatchSwaps(swaps);
    expect(result.cancelledCount).toBe(1);
    expect(result.failedCount).toBe(1);
  });

  test("totalAmount equals sum of all cancelled swap amounts", () => {
    const swaps = [pendingSwap("t1", 300), pendingSwap("t2", 700)];
    const result = cancelBatchSwaps(swaps);
    expect(result.totalAmount).toBe(1000);
  });
});
