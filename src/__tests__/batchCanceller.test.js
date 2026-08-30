const {
  cancelBatchSwaps,
  REFUND_POLICIES,
  CANCELLABLE_STATES,
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

// ── #875: refund-policy × cancellable-state enforcement matrix ────────────────

describe("cancelBatchSwaps — refund policy × state matrix", () => {
  const AMOUNT   = 1000;
  const FEE_PAID = 40;

  const expectedRefund = {
    [REFUND_POLICIES.FULL]:    AMOUNT,
    [REFUND_POLICIES.PARTIAL]: +(AMOUNT - FEE_PAID).toFixed(8),
    [REFUND_POLICIES.NONE]:    0,
  };

  const stateFactories = {
    PENDING: pendingSwap,
    ACTIVE:  activeSwap,
  };

  const cases = [];
  for (const state of Object.keys(stateFactories)) {
    for (const policy of Object.values(REFUND_POLICIES)) {
      cases.push([state, policy]);
    }
  }

  test.each(cases)(
    "%s swap under %s policy resolves to CANCELLED with the correct refund",
    (state, policy) => {
      const swap = stateFactories[state](`${state}-${policy}`, AMOUNT);
      const result = cancelBatchSwaps(
        [swap],
        [{ refundPolicy: policy, feePaid: FEE_PAID }]
      );

      expect(result.cancelledCount).toBe(1);
      expect(result.failedCount).toBe(0);
      expect(result.results[0].newState).toBe(CANCELLED_STATE);
      expect(result.results[0].refundPolicy).toBe(policy);
      expect(result.results[0].refundAmount).toBeCloseTo(expectedRefund[policy]);
    }
  );

  test("every CANCELLABLE_STATES entry is actually accepted by the batch", () => {
    for (const state of CANCELLABLE_STATES) {
      const result = cancelBatchSwaps([{ swapId: `cs-${state}`, state, amount: 10 }]);
      expect(result.cancelledCount).toBe(1);
    }
  });
});

// ── #875: partial-batch isolation for non-cancellable states ──────────────────

describe("cancelBatchSwaps — non-cancellable states in a partial batch", () => {
  test("a non-cancellable swap is logged as a per-item error without aborting the batch", () => {
    const swaps = [
      pendingSwap("ok-1", 100),
      { swapId: "bad-1", state: "COMPLETED", amount: 200 },
      activeSwap("ok-2", 300),
      { swapId: "bad-2", state: "EXPIRED", amount: 400 },
    ];

    const result = cancelBatchSwaps(swaps);

    expect(result.batchSize).toBe(4);
    expect(result.cancelledCount).toBe(2);
    expect(result.failedCount).toBe(2);

    const cancelledIds = result.results.map((r) => r.swapId);
    expect(cancelledIds).toEqual(["ok-1", "ok-2"]);

    const failedIds = result.errors.map((e) => e.swapId);
    expect(failedIds).toEqual(["bad-1", "bad-2"]);
    expect(result.errors[0].error).toMatch(/cannot cancel a swap in state 'COMPLETED'/);
    expect(result.errors[1].error).toMatch(/cannot cancel a swap in state 'EXPIRED'/);

    // Refund/amount totals only reflect the swaps that actually cancelled.
    expect(result.totalAmount).toBe(400);
  });

  test("every swap non-cancellable still returns zero cancelled rather than throwing", () => {
    const swaps = [
      { swapId: "all-bad-1", state: "COMPLETED", amount: 50 },
      { swapId: "all-bad-2", state: "CANCELLED", amount: 60 },
    ];
    const result = cancelBatchSwaps(swaps);
    expect(result.cancelledCount).toBe(0);
    expect(result.failedCount).toBe(2);
    expect(result.results).toEqual([]);
    expect(result.totalRefunded).toBe(0);
  });
});

// ── #875: MAX_REASON_LENGTH boundary ───────────────────────────────────────────

describe("cancelBatchSwaps — MAX_REASON_LENGTH boundary", () => {
  const MAX_REASON_LENGTH = 256;

  test("reason exactly at the max length is accepted", () => {
    const reason = "x".repeat(MAX_REASON_LENGTH);
    const result = cancelBatchSwaps([pendingSwap("bound-ok")], [{ reason }]);
    expect(result.failedCount).toBe(0);
    expect(result.results[0].reason).toBe(reason);
    expect(result.results[0].reason).toHaveLength(MAX_REASON_LENGTH);
  });

  test("reason one character over the max length is rejected as a per-item error", () => {
    const reason = "x".repeat(MAX_REASON_LENGTH + 1);
    const result = cancelBatchSwaps([pendingSwap("bound-over")], [{ reason }]);
    expect(result.failedCount).toBe(1);
    expect(result.cancelledCount).toBe(0);
    expect(result.errors[0].error).toMatch(
      new RegExp(`must not exceed ${MAX_REASON_LENGTH} characters`)
    );
  });

  test("an empty reason is always accepted", () => {
    const result = cancelBatchSwaps([pendingSwap("bound-empty")], [{ reason: "" }]);
    expect(result.failedCount).toBe(0);
    expect(result.results[0].reason).toBe("");
  });
});
