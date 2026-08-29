const {
  processBatchPartialPayments,
  applyPartialPayment,
  calculateRemainingInstallments,
  batchPaymentSummary,
  remainingBalance,
  installmentAmount,
  MAX_BATCH_SIZE,
} = require("../batch/batchPartialPayment");

const swap = (swapId, totalPrice, paidAmount = 0) => ({ swapId, totalPrice, paidAmount });

describe("batch partial payment — validation", () => {
  test("rejects an empty batch", () => {
    expect(() => processBatchPartialPayments([], 100)).toThrow(TypeError);
  });

  test("rejects a batch over MAX_BATCH_SIZE", () => {
    const swaps = Array.from({ length: MAX_BATCH_SIZE + 1 }, (_, i) => swap(`s${i}`, 1));
    expect(() => processBatchPartialPayments(swaps, MAX_BATCH_SIZE + 1)).toThrow(RangeError);
  });

  test("accepts a batch exactly at MAX_BATCH_SIZE", () => {
    const swaps = Array.from({ length: MAX_BATCH_SIZE }, (_, i) => swap(`s${i}`, 1));
    const result = processBatchPartialPayments(swaps, MAX_BATCH_SIZE);
    expect(result.batchSize).toBe(MAX_BATCH_SIZE);
    expect(result.completedCount).toBe(MAX_BATCH_SIZE);
  });

  test("rejects invalid payment balances", () => {
    expect(() => processBatchPartialPayments([swap("s1", 100)], -1)).toThrow(RangeError);
    expect(() => applyPartialPayment(swap("s1", 100), 0)).toThrow(RangeError);
  });
});

describe("applyPartialPayment", () => {
  test("fully applies an exact payment", () => {
    expect(applyPartialPayment(swap("exact", 100, 25), 75)).toEqual({
      swapId: "exact",
      appliedAmount: 75,
      newPaidAmount: 100,
      remaining: 0,
      status: "COMPLETED",
    });
  });

  test("leaves a swap pending for an under-payment", () => {
    expect(applyPartialPayment(swap("under", 100, 20), 30)).toEqual({
      swapId: "under",
      appliedAmount: 30,
      newPaidAmount: 50,
      remaining: 50,
      status: "PENDING",
    });
  });

  test("caps an over-payment at the amount owed", () => {
    expect(applyPartialPayment(swap("over", 100, 20), 200)).toEqual({
      swapId: "over",
      appliedAmount: 80,
      newPaidAmount: 100,
      remaining: 0,
      status: "COMPLETED",
    });
  });
});

describe("processBatchPartialPayments", () => {
  test("allocates exact, under-, and over-payment cases in order", () => {
    const result = processBatchPartialPayments(
      [swap("first", 100), swap("second", 50, 20), swap("third", 40)],
      180
    );

    expect(result.outcomes).toEqual([
      { swapId: "first", appliedAmount: 100, newPaidAmount: 100, remaining: 0, status: "COMPLETED" },
      { swapId: "second", appliedAmount: 50, newPaidAmount: 70, remaining: 0, status: "COMPLETED" },
      { swapId: "third", appliedAmount: 30, newPaidAmount: 30, remaining: 10, status: "PENDING" },
    ]);
    expect(result.totalApplied).toBe(180);
    expect(result.remainingBalance).toBe(0);
    expect(result.completedCount).toBe(2);
    expect(result.partialCount).toBe(1);
  });

  test("skips swaps after the available balance is exhausted", () => {
    const result = processBatchPartialPayments([swap("a", 100), swap("b", 100)], 100);

    expect(result.outcomes[0].status).toBe("COMPLETED");
    expect(result.outcomes[1]).toEqual({
      swapId: "b",
      appliedAmount: 0,
      newPaidAmount: 0,
      remaining: 100,
      status: "SKIPPED",
    });
    expect(result.skippedCount).toBe(1);
  });

  test("strict mode skips a swap that cannot be fully paid", () => {
    const result = processBatchPartialPayments([swap("strict", 100)], 40, { allowPartial: false });
    expect(result.outcomes[0].status).toBe("SKIPPED");
    expect(result.totalApplied).toBe(0);
    expect(result.remainingBalance).toBe(40);
  });
});

describe("partial payment helpers", () => {
  test("calculates remaining installments using ceiling arithmetic", () => {
    expect(calculateRemainingInstallments([swap("installments", 100, 15)], 30)).toEqual([
      { swapId: "installments", installmentsRemaining: 3, nextPaymentAmount: 30, owed: 85 },
    ]);
    expect(installmentAmount(swap("installment", 100), 3)).toBe(34);
  });

  test("summarizes total value, paid amount, and completion", () => {
    const result = batchPaymentSummary([swap("done", 100, 100), swap("open", 300, 75)]);

    expect(result).toMatchObject({
      batchSize: 2,
      totalValue: 400,
      totalPaid: 175,
      totalOwed: 225,
      completionPct: 43.75,
    });
    expect(result.swapSummaries).toEqual([
      expect.objectContaining({ swapId: "done", owed: 0, status: "COMPLETED" }),
      expect.objectContaining({ swapId: "open", owed: 225, status: "PENDING" }),
    ]);
    expect(remainingBalance(swap("open", 300, 75))).toBe(225);
  });
});