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

  test("MULTI_PARTY distributes the full amount across named participants", () => {
    const dispute = { ...openDispute("r-multi", 1000), initiator: "seller", counterparty: "buyer" };
    const result = resolveBatchDisputes(
      [dispute],
      [{
        type: RESOLUTION_TYPES.MULTI_PARTY,
        allocations: { seller: 500, buyer: 300, arbitrator: 200 },
      }]
    );

    expect(result.resolvedCount).toBe(1);
    expect(result.results[0].participantAmounts).toEqual({
      seller: 500,
      buyer: 300,
      arbitrator: 200,
    });
    expect(result.totalSplit).toBe(1000);
  });

  test("rejects multi-party allocations that do not balance", () => {
    const result = resolveBatchDisputes(
      [openDispute("r-unbalanced", 1000)],
      [{
        type: RESOLUTION_TYPES.MULTI_PARTY,
        allocations: { seller: 600, buyer: 300 },
      }]
    );

    expect(result.failedCount).toBe(1);
    expect(result.errors[0].error).toMatch(/allocations must total 1000/);
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

describe("resolveBatchDisputes — SPLIT in integer basis points (#1264)", () => {
  const split = (fields, amount = 10000) =>
    resolveBatchDisputes(
      [openDispute("bps", amount)],
      [{ type: RESOLUTION_TYPES.SPLIT, ...fields }]
    );

  test("splitBps accepts the lower bound 100 (1%)", () => {
    const result = split({ splitBps: 100 });
    expect(result.failedCount).toBe(0);
    expect(result.results[0].splitBps).toBe(100);
    expect(result.results[0].initiatorAmount).toBe(100);
    expect(result.results[0].counterpartyAmount).toBe(9900);
  });

  test("splitBps accepts the upper bound 9900 (99%)", () => {
    const result = split({ splitBps: 9900 });
    expect(result.failedCount).toBe(0);
    expect(result.results[0].initiatorAmount).toBe(9900);
    expect(result.results[0].counterpartyAmount).toBe(100);
  });

  test.each([0, 99, 9901, 10000])("splitBps %i outside [100, 9900] is rejected", (bps) => {
    const result = split({ splitBps: bps });
    expect(result.failedCount).toBe(1);
    expect(result.errors[0].error).toMatch(/splitBps must be an integer between 100 and 9900/);
  });

  test.each([5000.5, "5000", NaN])("non-integer or non-numeric splitBps %p is rejected", (bps) => {
    const result = split({ splitBps: bps });
    expect(result.failedCount).toBe(1);
    expect(result.errors[0].error).toMatch(/splitBps must be an integer/);
  });

  test("default split is 5000 bps (50/50)", () => {
    const result = split({});
    expect(result.results[0].splitBps).toBe(5000);
    expect(result.results[0].initiatorAmount).toBe(5000);
  });

  test("legacy splitRatio 0.1 + 0.2 is accepted as exactly 3000 bps", () => {
    const result = split({ splitRatio: 0.1 + 0.2 });
    expect(result.failedCount).toBe(0);
    expect(result.results[0].splitBps).toBe(3000);
    expect(result.results[0].initiatorAmount).toBe(3000);
  });

  test("legacy splitRatio at the bounds 0.01 and 0.99 is accepted", () => {
    expect(split({ splitRatio: 0.01 }).results[0].splitBps).toBe(100);
    expect(split({ splitRatio: 0.99 }).results[0].splitBps).toBe(9900);
  });

  test("legacy splitRatio that is not a whole number of basis points is rejected", () => {
    const result = split({ splitRatio: 0.12345 });
    expect(result.failedCount).toBe(1);
    expect(result.errors[0].error).toMatch(/whole number of basis points/);
  });

  test("legacy splitRatio outside [0.01, 0.99] is rejected", () => {
    expect(split({ splitRatio: 0.009 }).failedCount).toBe(1);
    expect(split({ splitRatio: 0.991 }).failedCount).toBe(1);
  });
});
