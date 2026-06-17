const {
  resolveBatchDisputes,
  DISPUTE_STATES,
  RESOLUTION_TYPES,
  MAX_BATCH_SIZE,
} = require("../batch/batchDisputeResolver");

const openDispute = (id, amount = 1000) => ({
  swapId: id, state: DISPUTE_STATES.OPEN, amount,
});

describe("resolveBatchDisputes — validation", () => {
  test("throws on empty disputes array", () => {
    expect(() => resolveBatchDisputes([], [])).toThrow(TypeError);
  });
  test("throws when arrays length mismatch", () => {
    expect(() => resolveBatchDisputes([openDispute("a")], [])).toThrow(TypeError);
  });
  test("throws on batch > MAX_BATCH_SIZE", () => {
    const big = Array.from({ length: MAX_BATCH_SIZE + 1 }, (_, i) => openDispute(`s${i}`));
    const res = big.map(() => ({ type: RESOLUTION_TYPES.REFUND }));
    expect(() => resolveBatchDisputes(big, res)).toThrow(RangeError);
  });
  test("records error for non-OPEN dispute without throwing", () => {
    const d = [{ swapId: "x", state: DISPUTE_STATES.RESOLVED, amount: 100 }];
    const r = [{ type: RESOLUTION_TYPES.REFUND }];
    const result = resolveBatchDisputes(d, r);
    expect(result.failedCount).toBe(1);
    expect(result.errors[0].swapId).toBe("x");
  });
});

describe("resolveBatchDisputes — resolution types", () => {
  test("REFUND returns full amount to initiator", () => {
    const result = resolveBatchDisputes(
      [openDispute("r1", 500)],
      [{ type: RESOLUTION_TYPES.REFUND }]
    );
    expect(result.resolvedCount).toBe(1);
    expect(result.results[0].initiatorAmount).toBe(500);
    expect(result.results[0].counterpartyAmount).toBe(0);
    expect(result.totalRefunded).toBe(500);
  });

  test("RELEASE sends full amount to counterparty", () => {
    const result = resolveBatchDisputes(
      [openDispute("r2", 750)],
      [{ type: RESOLUTION_TYPES.RELEASE }]
    );
    expect(result.results[0].counterpartyAmount).toBe(750);
    expect(result.results[0].initiatorAmount).toBe(0);
    expect(result.totalReleased).toBe(750);
  });

  test("SPLIT with default ratio distributes 50/50", () => {
    const result = resolveBatchDisputes(
      [openDispute("r3", 1000)],
      [{ type: RESOLUTION_TYPES.SPLIT }]
    );
    expect(result.results[0].initiatorAmount).toBeCloseTo(500);
    expect(result.results[0].counterpartyAmount).toBeCloseTo(500);
  });

  test("SPLIT with custom ratio respects ratio", () => {
    const result = resolveBatchDisputes(
      [openDispute("r4", 1000)],
      [{ type: RESOLUTION_TYPES.SPLIT, splitRatio: 0.7 }]
    );
    expect(result.results[0].initiatorAmount).toBeCloseTo(700);
    expect(result.results[0].counterpartyAmount).toBeCloseTo(300);
  });

  test("ESCALATE moves state to ESCALATED", () => {
    const result = resolveBatchDisputes(
      [openDispute("r5")],
      [{ type: RESOLUTION_TYPES.ESCALATE, reason: "Complex case" }]
    );
    expect(result.escalatedCount).toBe(1);
    expect(result.results[0].newState).toBe(DISPUTE_STATES.ESCALATED);
  });
});

describe("resolveBatchDisputes — mixed batch", () => {
  test("processes mixed outcomes and counts correctly", () => {
    const disputes = [
      openDispute("m1", 100),
      openDispute("m2", 200),
      { swapId: "m3", state: DISPUTE_STATES.RESOLVED, amount: 300 }, // will fail
    ];
    const resolutions = [
      { type: RESOLUTION_TYPES.REFUND },
      { type: RESOLUTION_TYPES.RELEASE },
      { type: RESOLUTION_TYPES.REFUND },
    ];
    const result = resolveBatchDisputes(disputes, resolutions);
    expect(result.resolvedCount).toBe(2);
    expect(result.failedCount).toBe(1);
    expect(result.totalRefunded).toBe(100);
    expect(result.totalReleased).toBe(200);
  });
});
