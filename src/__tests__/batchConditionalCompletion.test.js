const {
  processBatchConditionalCompletion,
  evaluateSwapConditions,
  evaluateCondition,
  filterEligibleSwaps,
  isSwapEligible,
  ConditionType,
  MAX_BATCH_SIZE,
} = require("../batch/batchConditionalCompletion");

const validSwap = (id, price = 100, conditions = [], overrides = {}) => ({
  swapId: id,
  price,
  conditions,
  ...overrides,
});

const NOW = 1_700_000_000_000;

describe("processBatchConditionalCompletion — input validation", () => {
  test("throws TypeError when swaps is not an array", () => {
    expect(() => processBatchConditionalCompletion(null)).toThrow(TypeError);
    expect(() => processBatchConditionalCompletion("not-an-array")).toThrow(TypeError);
    expect(() => processBatchConditionalCompletion({})).toThrow(TypeError);
  });

  test("throws TypeError on empty swaps array", () => {
    expect(() => processBatchConditionalCompletion([])).toThrow(TypeError);
  });

  test("throws RangeError when batch exceeds MAX_BATCH_SIZE", () => {
    const big = Array.from({ length: MAX_BATCH_SIZE + 1 }, (_, i) =>
      validSwap(`s-${i}`)
    );
    expect(() => processBatchConditionalCompletion(big)).toThrow(RangeError);
  });

  test("throws TypeError when an entry is not an object", () => {
    expect(() => processBatchConditionalCompletion(["invalid"])).toThrow(TypeError);
  });

  test("throws TypeError when swapId is missing or empty", () => {
    expect(() =>
      processBatchConditionalCompletion([{ price: 100, conditions: [] }])
    ).toThrow(TypeError);
    expect(() =>
      processBatchConditionalCompletion([{ swapId: "", price: 100, conditions: [] }])
    ).toThrow(TypeError);
  });

  test("throws RangeError when price is non-positive or non-number", () => {
    expect(() =>
      processBatchConditionalCompletion([validSwap("s1", 0)])
    ).toThrow(RangeError);
    expect(() =>
      processBatchConditionalCompletion([validSwap("s1", -10)])
    ).toThrow(RangeError);
    expect(() =>
      processBatchConditionalCompletion([{ swapId: "s1", price: "100", conditions: [] }])
    ).toThrow(RangeError);
  });

  test("throws TypeError when conditions is not an array", () => {
    expect(() =>
      processBatchConditionalCompletion([{ swapId: "s1", price: 100, conditions: null }])
    ).toThrow(TypeError);
  });
});

describe("evaluateCondition — single condition evaluation", () => {
  describe("KEY_VALID", () => {
    test("passes when keyHash matches expectedKeyHash", () => {
      const cond = { type: ConditionType.KEY_VALID, expectedKeyHash: "hash-123" };
      const swap = validSwap("s1", 100, [], { keyHash: "hash-123" });
      const res = evaluateCondition(cond, swap);
      expect(res.passed).toBe(true);
      expect(res.reason).toBe("key valid");
    });

    test("fails when keyHash does not match", () => {
      const cond = { type: ConditionType.KEY_VALID, expectedKeyHash: "hash-123" };
      const swap = validSwap("s1", 100, [], { keyHash: "wrong-hash" });
      const res = evaluateCondition(cond, swap);
      expect(res.passed).toBe(false);
      expect(res.reason).toBe("key hash mismatch or missing");
    });

    test("fails when keyHash is missing", () => {
      const cond = { type: ConditionType.KEY_VALID, expectedKeyHash: "hash-123" };
      const swap = validSwap("s1", 100, []);
      const res = evaluateCondition(cond, swap);
      expect(res.passed).toBe(false);
      expect(res.reason).toBe("key hash mismatch or missing");
    });
  });

  describe("PRICE_BELOW", () => {
    test("passes when price is strictly below threshold", () => {
      const cond = { type: ConditionType.PRICE_BELOW, threshold: 200 };
      const swap = validSwap("s1", 150);
      const res = evaluateCondition(cond, swap);
      expect(res.passed).toBe(true);
      expect(res.reason).toContain("price 150 < 200");
    });

    test("fails when price equals or exceeds threshold", () => {
      const cond = { type: ConditionType.PRICE_BELOW, threshold: 100 };
      const swapEqual = validSwap("s1", 100);
      const swapHigher = validSwap("s2", 150);

      expect(evaluateCondition(cond, swapEqual).passed).toBe(false);
      expect(evaluateCondition(cond, swapHigher).passed).toBe(false);
    });

    test("throws TypeError when threshold is non-numeric", () => {
      const cond = { type: ConditionType.PRICE_BELOW, threshold: "200" };
      const swap = validSwap("s1", 100);
      expect(() => evaluateCondition(cond, swap)).toThrow(TypeError);
    });
  });

  describe("TIME_AFTER", () => {
    test("passes when current time is at or after afterMs", () => {
      const cond = { type: ConditionType.TIME_AFTER, afterMs: NOW - 1000 };
      const swap = validSwap("s1", 100);
      const res = evaluateCondition(cond, swap, { nowMs: NOW });
      expect(res.passed).toBe(true);
      expect(res.reason).toContain(`now (${NOW}) >= ${NOW - 1000}`);
    });

    test("passes when current time exactly equals afterMs", () => {
      const cond = { type: ConditionType.TIME_AFTER, afterMs: NOW };
      const swap = validSwap("s1", 100);
      const res = evaluateCondition(cond, swap, { nowMs: NOW });
      expect(res.passed).toBe(true);
    });

    test("fails when current time is before afterMs", () => {
      const cond = { type: ConditionType.TIME_AFTER, afterMs: NOW + 1000 };
      const swap = validSwap("s1", 100);
      const res = evaluateCondition(cond, swap, { nowMs: NOW });
      expect(res.passed).toBe(false);
      expect(res.reason).toContain(`now (${NOW}) < ${NOW + 1000}`);
    });

    test("uses Date.now() when ctx.nowMs is not provided", () => {
      const cond = { type: ConditionType.TIME_AFTER, afterMs: 0 };
      const swap = validSwap("s1", 100);
      const res = evaluateCondition(cond, swap);
      expect(res.passed).toBe(true);
    });

    test("throws TypeError when afterMs is not a number", () => {
      const cond = { type: ConditionType.TIME_AFTER, afterMs: "1000" };
      const swap = validSwap("s1", 100);
      expect(() => evaluateCondition(cond, swap)).toThrow(TypeError);
    });
  });

  describe("CUSTOM", () => {
    test("passes when custom predicate returns truthy value", () => {
      const cond = {
        type: ConditionType.CUSTOM,
        predicate: (s) => s.price % 10 === 0,
      };
      const swap = validSwap("s1", 100);
      const res = evaluateCondition(cond, swap);
      expect(res.passed).toBe(true);
      expect(res.reason).toBe("custom predicate passed");
    });

    test("fails when custom predicate returns falsy value", () => {
      const cond = {
        type: ConditionType.CUSTOM,
        predicate: (s, ctx) => ctx.role === "admin",
      };
      const swap = validSwap("s1", 100);
      const res = evaluateCondition(cond, swap, { role: "guest" });
      expect(res.passed).toBe(false);
      expect(res.reason).toBe("custom predicate failed");
    });

    test("throws TypeError when predicate is not a function", () => {
      const cond = { type: ConditionType.CUSTOM, predicate: true };
      const swap = validSwap("s1", 100);
      expect(() => evaluateCondition(cond, swap)).toThrow(TypeError);
    });
  });
});

describe("evaluateSwapConditions — compound conditions", () => {
  test("swap with no conditions is eligible by default", () => {
    const swap = validSwap("s1", 100, []);
    const { eligible, conditionResults } = evaluateSwapConditions(swap);
    expect(eligible).toBe(true);
    expect(conditionResults).toEqual([]);
  });

  test("swap is eligible when all multiple conditions pass", () => {
    const conditions = [
      { type: ConditionType.PRICE_BELOW, threshold: 200 },
      { type: ConditionType.KEY_VALID, expectedKeyHash: "k1" },
      { type: ConditionType.TIME_AFTER, afterMs: NOW - 500 },
    ];
    const swap = validSwap("s1", 150, conditions, { keyHash: "k1" });
    const { eligible, conditionResults } = evaluateSwapConditions(swap, { nowMs: NOW });
    expect(eligible).toBe(true);
    expect(conditionResults).toHaveLength(3);
    expect(conditionResults.every((r) => r.passed)).toBe(true);
  });

  test("swap is ineligible if any condition fails", () => {
    const conditions = [
      { type: ConditionType.PRICE_BELOW, threshold: 100 }, // fails (price=150)
      { type: ConditionType.KEY_VALID, expectedKeyHash: "k1" }, // passes
    ];
    const swap = validSwap("s1", 150, conditions, { keyHash: "k1" });
    const { eligible, conditionResults } = evaluateSwapConditions(swap);
    expect(eligible).toBe(false);
    expect(conditionResults[0].passed).toBe(false);
    expect(conditionResults[1].passed).toBe(true);
  });

  test("throws when a condition is not an object or has unknown type", () => {
    const invalidCondSwap = validSwap("s1", 100, ["not-an-object"]);
    expect(() => evaluateSwapConditions(invalidCondSwap)).toThrow(TypeError);

    const unknownTypeSwap = validSwap("s2", 100, [{ type: "INVALID_TYPE" }]);
    expect(() => evaluateSwapConditions(unknownTypeSwap)).toThrow(TypeError);
  });
});

describe("processBatchConditionalCompletion — execution and partial-batch success", () => {
  test("processes all eligible swaps to COMPLETED status", () => {
    const swaps = [
      validSwap("s1", 50, [{ type: ConditionType.PRICE_BELOW, threshold: 100 }]),
      validSwap("s2", 80, [{ type: ConditionType.PRICE_BELOW, threshold: 100 }]),
    ];
    const result = processBatchConditionalCompletion(swaps, { nowMs: NOW });
    expect(result.batchSize).toBe(2);
    expect(result.completed).toBe(2);
    expect(result.skipped).toBe(0);
    expect(result.failed).toBe(0);
    expect(result.errors).toHaveLength(0);
    expect(result.results[0].status).toBe("COMPLETED");
    expect(result.results[0].completedAt).toBe(NOW);
    expect(result.results[1].status).toBe("COMPLETED");
    expect(result.results[1].completedAt).toBe(NOW);
  });

  test("partial-batch success: marks satisfying swaps COMPLETED and failing swaps SKIPPED", () => {
    const swaps = [
      validSwap("s1", 50, [{ type: ConditionType.PRICE_BELOW, threshold: 100 }]), // eligible
      validSwap("s2", 150, [{ type: ConditionType.PRICE_BELOW, threshold: 100 }]), // ineligible
      validSwap("s3", 30, [], { keyHash: "k3" }), // eligible (no conditions)
    ];
    const result = processBatchConditionalCompletion(swaps, { nowMs: NOW });

    expect(result.batchSize).toBe(3);
    expect(result.completed).toBe(2);
    expect(result.skipped).toBe(1);
    expect(result.failed).toBe(0);

    expect(result.results[0].swapId).toBe("s1");
    expect(result.results[0].status).toBe("COMPLETED");
    expect(result.results[0].completedAt).toBe(NOW);

    expect(result.results[1].swapId).toBe("s2");
    expect(result.results[1].status).toBe("SKIPPED");
    expect(result.results[1].completedAt).toBeNull();

    expect(result.results[2].swapId).toBe("s3");
    expect(result.results[2].status).toBe("COMPLETED");
    expect(result.results[2].completedAt).toBe(NOW);
  });

  test("handles batch where all swaps are SKIPPED", () => {
    const swaps = [
      validSwap("s1", 200, [{ type: ConditionType.PRICE_BELOW, threshold: 100 }]),
      validSwap("s2", 300, [{ type: ConditionType.PRICE_BELOW, threshold: 100 }]),
    ];
    const result = processBatchConditionalCompletion(swaps);
    expect(result.completed).toBe(0);
    expect(result.skipped).toBe(2);
    expect(result.failed).toBe(0);
  });
});

describe("processBatchConditionalCompletion — invalid condition error handling", () => {
  test("records error and increments failed count when condition format is invalid", () => {
    const swaps = [
      validSwap("s1", 50, [{ type: ConditionType.PRICE_BELOW, threshold: 100 }]), // valid
      validSwap("s2", 50, [{ type: "UNKNOWN_TYPE" }]), // invalid condition type
      validSwap("s3", 50, [{ type: ConditionType.PRICE_BELOW, threshold: "invalid" }]), // invalid threshold type
    ];

    const result = processBatchConditionalCompletion(swaps);
    expect(result.batchSize).toBe(3);
    expect(result.completed).toBe(1);
    expect(result.failed).toBe(2);
    expect(result.errors).toHaveLength(2);
    expect(result.errors[0].swapId).toBe("s2");
    expect(result.errors[1].swapId).toBe("s3");
  });
});

describe("filterEligibleSwaps and isSwapEligible", () => {
  const swaps = [
    validSwap("s1", 50, [{ type: ConditionType.PRICE_BELOW, threshold: 100 }]),
    validSwap("s2", 150, [{ type: ConditionType.PRICE_BELOW, threshold: 100 }]),
    validSwap("s3", 75, [{ type: ConditionType.PRICE_BELOW, threshold: 100 }]),
  ];

  test("filterEligibleSwaps returns only swaps satisfying conditions", () => {
    const eligible = filterEligibleSwaps(swaps);
    expect(eligible).toHaveLength(2);
    expect(eligible.map((s) => s.swapId)).toEqual(["s1", "s3"]);
  });

  test("filterEligibleSwaps throws TypeError on non-array", () => {
    expect(() => filterEligibleSwaps(null)).toThrow(TypeError);
  });

  test("filterEligibleSwaps safely filters out swaps with errors", () => {
    const withError = [
      validSwap("s1", 50, [{ type: ConditionType.PRICE_BELOW, threshold: 100 }]),
      validSwap("bad", 50, [{ type: "UNKNOWN_TYPE" }]),
    ];
    const eligible = filterEligibleSwaps(withError);
    expect(eligible).toHaveLength(1);
    expect(eligible[0].swapId).toBe("s1");
  });

  test("isSwapEligible returns true when eligible and false when not", () => {
    expect(isSwapEligible(swaps[0])).toBe(true);
    expect(isSwapEligible(swaps[1])).toBe(false);
  });

  test("isSwapEligible returns false when swap has error", () => {
    const badSwap = validSwap("bad", 50, [{ type: "UNKNOWN_TYPE" }]);
    expect(isSwapEligible(badSwap)).toBe(false);
  });
});
