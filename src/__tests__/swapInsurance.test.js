const {
  calculatePremium,
  issuePolicy,
  fileClaim,
  assessRiskFactor,
  evaluateDisputePayout,
  processDisputeResolutions,
  POLICY_TYPES,
  COVERAGE_EVENTS,
  CLAIM_STATUSES,
  DEDUCTIBLE_RATIO,
} = require("../insurance/swapInsurance");

const {
  resolveBatchDisputes,
  RESOLUTION_TYPES,
} = require("../batch/batchDisputeResolver");

describe("assessRiskFactor", () => {
  test("baseline risk factor is ~1.0", () => {
    expect(assessRiskFactor({ swapValue: 1000 })).toBeCloseTo(1.0, 1);
  });
  test("low reputation increases risk", () => {
    expect(assessRiskFactor({ swapValue: 1000, sellerReputationScore: 200 })).toBeGreaterThan(1.0);
  });
  test("high reputation decreases risk", () => {
    expect(assessRiskFactor({ swapValue: 1000, sellerReputationScore: 900 })).toBeLessThan(1.0);
  });
  test("risk factor clamped to [0.5, 3.0]", () => {
    const worst = assessRiskFactor({ swapValue: 500_000, sellerReputationScore: 0, sellerSwapCount: 0 });
    expect(worst).toBeLessThanOrEqual(3.0);
    const best = assessRiskFactor({ swapValue: 1, sellerReputationScore: 1000, sellerSwapCount: 100 });
    expect(best).toBeGreaterThanOrEqual(0.5);
  });
});

describe("calculatePremium", () => {
  test("throws on non-positive swapValue", () => {
    expect(() => calculatePremium(0, POLICY_TYPES.BASIC)).toThrow(RangeError);
    expect(() => calculatePremium(-100, POLICY_TYPES.BASIC)).toThrow(RangeError);
  });
  test("throws on invalid policyType", () => {
    expect(() => calculatePremium(1000, "ULTRA")).toThrow(TypeError);
  });
  test("premium increases BASIC → STANDARD → PREMIUM", () => {
    const meta = { sellerReputationScore: 500, sellerSwapCount: 10 };
    const b = calculatePremium(10_000, POLICY_TYPES.BASIC,    meta);
    const s = calculatePremium(10_000, POLICY_TYPES.STANDARD, meta);
    const p = calculatePremium(10_000, POLICY_TYPES.PREMIUM,  meta);
    expect(s.premium).toBeGreaterThan(b.premium);
    expect(p.premium).toBeGreaterThan(s.premium);
  });
  test("coverageAmount equals swapValue", () => {
    const { coverageAmount, swapValue } = calculatePremium(5_000, POLICY_TYPES.STANDARD);
    expect(coverageAmount).toBe(swapValue);
  });
});

describe("issuePolicy", () => {
  const req = { swapId: "swap-1", policyType: POLICY_TYPES.PREMIUM, swapValue: 10_000, buyerId: "buyer-1" };

  test("issues active policy with correct covered events", () => {
    const policy = issuePolicy(req);
    expect(policy.coveredEvents).toContain(COVERAGE_EVENTS.FRAUD);
    expect(policy.status).toBe("ACTIVE");
  });
  test("throws if swapId missing", () => {
    expect(() => issuePolicy({ ...req, swapId: undefined })).toThrow(TypeError);
  });
  test("BASIC policy does not cover FRAUD", () => {
    const p = issuePolicy({ ...req, policyType: POLICY_TYPES.BASIC });
    expect(p.coveredEvents).not.toContain(COVERAGE_EVENTS.FRAUD);
  });
});

describe("fileClaim", () => {
  const policy = issuePolicy({
    swapId: "swap-2", policyType: POLICY_TYPES.STANDARD, swapValue: 10_000, buyerId: "b1",
  });

  test("rejects claim for uncovered event", () => {
    const result = fileClaim(policy, { event: COVERAGE_EVENTS.FRAUD, claimedAmount: 5000, evidence: "proof" });
    expect(result.status).toBe(CLAIM_STATUSES.REJECTED);
  });

  test("pends claim with no evidence", () => {
    const result = fileClaim(policy, { event: COVERAGE_EVENTS.NON_DELIVERY, claimedAmount: 5000, evidence: "" });
    expect(result.status).toBe(CLAIM_STATUSES.PENDING);
  });

  test("approves valid claim with deductible", () => {
    const result = fileClaim(policy, { event: COVERAGE_EVENTS.NON_DELIVERY, claimedAmount: 5000, evidence: "screenshot" });
    expect(result.status).toBe(CLAIM_STATUSES.APPROVED);
    expect(result.deductible).toBeCloseTo(5000 * DEDUCTIBLE_RATIO, 2);
    expect(result.payout).toBeCloseTo(5000 * (1 - DEDUCTIBLE_RATIO), 2);
  });

  test("caps payout at coverageAmount", () => {
    const result = fileClaim(policy, { event: COVERAGE_EVENTS.NON_DELIVERY, claimedAmount: 999_999, evidence: "proof" });
    expect(result.coverageApplied).toBe(policy.coverageAmount);
  });

  test("rejects claim against inactive policy", () => {
    const cancelled = { ...policy, status: "CANCELLED" };
    const result = fileClaim(cancelled, { event: COVERAGE_EVENTS.NON_DELIVERY, claimedAmount: 100, evidence: "proof" });
    expect(result.status).toBe(CLAIM_STATUSES.REJECTED);
  });
});

// ── #876: dispute-resolution → insurance payout integration ───────────────────

describe("evaluateDisputePayout", () => {
  const buildPolicy = (overrides = {}) =>
    issuePolicy({
      swapId: "swap-dispute-1",
      policyType: POLICY_TYPES.STANDARD,
      swapValue: 10_000,
      buyerId: "buyer-1",
      ...overrides,
    });

  test("full dispute → payout flow: RELEASE against the buyer triggers an approved claim", () => {
    const policy = buildPolicy();
    const dispute = { swapId: policy.swapId, state: "OPEN", amount: 10_000 };
    const { results } = resolveBatchDisputes([dispute], [{ type: RESOLUTION_TYPES.RELEASE }]);
    const resolution = results[0];

    const outcome = evaluateDisputePayout(policy, resolution);

    expect(outcome.triggered).toBe(true);
    expect(outcome.shortfall).toBe(10_000);
    expect(outcome.claim.status).toBe(CLAIM_STATUSES.APPROVED);
    expect(outcome.claim.coverageApplied).toBe(policy.coverageAmount);
  });

  test("REFUND resolution does not trigger a payout (buyer already made whole via escrow)", () => {
    const policy = buildPolicy();
    const dispute = { swapId: policy.swapId, state: "OPEN", amount: 10_000 };
    const { results } = resolveBatchDisputes([dispute], [{ type: RESOLUTION_TYPES.REFUND }]);

    const outcome = evaluateDisputePayout(policy, results[0]);

    expect(outcome.triggered).toBe(false);
    expect(outcome.reason).toMatch(/no payout needed/i);
  });

  test("ESCALATE resolution does not trigger a payout (no final ruling yet)", () => {
    const policy = buildPolicy();
    const dispute = { swapId: policy.swapId, state: "OPEN", amount: 10_000 };
    const { results } = resolveBatchDisputes([dispute], [{ type: RESOLUTION_TYPES.ESCALATE }]);

    const outcome = evaluateDisputePayout(policy, results[0]);

    expect(outcome.triggered).toBe(false);
    expect(outcome.reason).toMatch(/ESCALATED/);
  });

  test("SPLIT resolution triggers a payout for only the buyer's shortfall", () => {
    const policy = buildPolicy();
    const dispute = { swapId: policy.swapId, state: "OPEN", amount: 10_000 };
    const { results } = resolveBatchDisputes(
      [dispute],
      [{ type: RESOLUTION_TYPES.SPLIT, splitRatio: 0.3 }] // buyer keeps 30%, 70% shortfall
    );

    const outcome = evaluateDisputePayout(policy, results[0]);

    expect(outcome.triggered).toBe(true);
    expect(outcome.shortfall).toBeCloseTo(7_000);
    expect(outcome.claim.status).toBe(CLAIM_STATUSES.APPROVED);
  });

  test("does not trigger when the resolution concerns a different swap", () => {
    const policy = buildPolicy();
    const outcome = evaluateDisputePayout(policy, {
      swapId: "other-swap",
      newState: "RESOLVED",
      resolutionType: RESOLUTION_TYPES.RELEASE,
      initiatorAmount: 0,
      counterpartyAmount: 10_000,
    });
    expect(outcome.triggered).toBe(false);
    expect(outcome.reason).toMatch(/does not concern this policy/i);
  });

  test("does not trigger against an inactive policy", () => {
    const policy = { ...buildPolicy(), status: "CANCELLED" };
    const outcome = evaluateDisputePayout(policy, {
      swapId: policy.swapId,
      newState: "RESOLVED",
      resolutionType: RESOLUTION_TYPES.RELEASE,
      initiatorAmount: 0,
      counterpartyAmount: 10_000,
    });
    expect(outcome.triggered).toBe(false);
    expect(outcome.reason).toMatch(/not active/i);
  });

  test("uses the arbitration ruling itself as evidence, so the claim is approved rather than pended", () => {
    const policy = buildPolicy();
    const outcome = evaluateDisputePayout(policy, {
      swapId: policy.swapId,
      newState: "RESOLVED",
      resolutionType: RESOLUTION_TYPES.RELEASE,
      initiatorAmount: 0,
      counterpartyAmount: 10_000,
    });
    expect(outcome.claim.status).not.toBe(CLAIM_STATUSES.PENDING);
  });
});

describe("processDisputeResolutions", () => {
  test("applies a batch of dispute resolutions across multiple policies by swapId", () => {
    const policyA = issuePolicy({ swapId: "swap-a", policyType: POLICY_TYPES.PREMIUM, swapValue: 5_000, buyerId: "b1" });
    const policyB = issuePolicy({ swapId: "swap-b", policyType: POLICY_TYPES.PREMIUM, swapValue: 8_000, buyerId: "b2" });

    const disputes = [
      { swapId: "swap-a", state: "OPEN", amount: 5_000 },
      { swapId: "swap-b", state: "OPEN", amount: 8_000 },
    ];
    const { results } = resolveBatchDisputes(disputes, [
      { type: RESOLUTION_TYPES.RELEASE },
      { type: RESOLUTION_TYPES.REFUND },
    ]);

    const outcomes = processDisputeResolutions([policyA, policyB], results);

    expect(outcomes).toHaveLength(2);
    expect(outcomes[0].swapId).toBe("swap-a");
    expect(outcomes[0].triggered).toBe(true);
    expect(outcomes[1].swapId).toBe("swap-b");
    expect(outcomes[1].triggered).toBe(false);
  });

  test("reports no policy found for a swap with no matching policy", () => {
    const outcomes = processDisputeResolutions([], [
      { swapId: "orphan-swap", newState: "RESOLVED", resolutionType: RESOLUTION_TYPES.RELEASE, initiatorAmount: 0, counterpartyAmount: 100 },
    ]);
    expect(outcomes[0].triggered).toBe(false);
    expect(outcomes[0].reason).toMatch(/no insurance policy found/i);
  });
});
