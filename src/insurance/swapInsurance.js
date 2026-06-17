/**
 * Swap Insurance — Issue #473
 * ────────────────────────────
 * Offers insurance policies for swap transactions.
 *
 * Policy types:
 *  - BASIC:    covers non-delivery
 *  - STANDARD: covers non-delivery + item-not-as-described
 *  - PREMIUM:  covers non-delivery + INAD + fraud + force majeure
 *
 * Premium = base_rate × coverage_multiplier × risk_factor × value
 */

const POLICY_TYPES = Object.freeze({
  BASIC:    "BASIC",
  STANDARD: "STANDARD",
  PREMIUM:  "PREMIUM",
});

const COVERAGE_EVENTS = Object.freeze({
  NON_DELIVERY:          "NON_DELIVERY",
  ITEM_NOT_AS_DESCRIBED: "ITEM_NOT_AS_DESCRIBED",
  FRAUD:                 "FRAUD",
  FORCE_MAJEURE:         "FORCE_MAJEURE",
});

const POLICY_COVERAGE = Object.freeze({
  [POLICY_TYPES.BASIC]:    new Set([COVERAGE_EVENTS.NON_DELIVERY]),
  [POLICY_TYPES.STANDARD]: new Set([COVERAGE_EVENTS.NON_DELIVERY, COVERAGE_EVENTS.ITEM_NOT_AS_DESCRIBED]),
  [POLICY_TYPES.PREMIUM]:  new Set(Object.values(COVERAGE_EVENTS)),
});

const BASE_RATE = 0.01;
const POLICY_MULTIPLIERS = Object.freeze({
  [POLICY_TYPES.BASIC]:    1.0,
  [POLICY_TYPES.STANDARD]: 1.6,
  [POLICY_TYPES.PREMIUM]:  2.5,
});

const CLAIM_STATUSES = Object.freeze({
  PENDING:  "PENDING",
  APPROVED: "APPROVED",
  REJECTED: "REJECTED",
  PAID:     "PAID",
});

const MAX_COVERAGE_RATIO = 1.0;
const DEDUCTIBLE_RATIO   = 0.05;

function assessRiskFactor(swapMeta) {
  let factor = 1.0;

  if (swapMeta.sellerReputationScore != null) {
    const rep = swapMeta.sellerReputationScore;
    if (rep < 300)      factor += 1.2;
    else if (rep < 500) factor += 0.6;
    else if (rep < 700) factor += 0.2;
    else                factor -= 0.2;
  }

  if (swapMeta.swapValue > 50_000)  factor += 0.5;
  if (swapMeta.swapValue > 200_000) factor += 0.5;

  if (swapMeta.sellerSwapCount != null && swapMeta.sellerSwapCount < 5)
    factor += 0.4;

  return Math.max(0.5, Math.min(3.0, +factor.toFixed(2)));
}

/**
 * Calculate insurance premium for a swap.
 *
 * @param {number} swapValue
 * @param {string} policyType - BASIC | STANDARD | PREMIUM
 * @param {object} [swapMeta]
 * @returns {{ premium, riskFactor, coverageAmount, policyType, swapValue }}
 */
function calculatePremium(swapValue, policyType, swapMeta = {}) {
  if (typeof swapValue !== "number" || swapValue <= 0)
    throw new RangeError("swapValue must be a positive number.");
  if (!Object.values(POLICY_TYPES).includes(policyType))
    throw new TypeError(`Invalid policyType: '${policyType}'.`);

  const riskFactor     = assessRiskFactor({ swapValue, ...swapMeta });
  const multiplier     = POLICY_MULTIPLIERS[policyType];
  const premium        = +(swapValue * BASE_RATE * multiplier * riskFactor).toFixed(2);
  const coverageAmount = +(swapValue * MAX_COVERAGE_RATIO).toFixed(2);

  return { premium, riskFactor, coverageAmount, policyType, swapValue };
}

/**
 * Issue an insurance policy for a swap.
 *
 * @param {{ swapId, policyType, swapValue, buyerId, swapMeta? }} req
 * @returns {InsurancePolicy}
 */
function issuePolicy(req) {
  const { swapId, policyType, swapValue, buyerId, swapMeta = {} } = req;
  if (!swapId)  throw new TypeError("swapId is required.");
  if (!buyerId) throw new TypeError("buyerId is required.");

  const { premium, riskFactor, coverageAmount } = calculatePremium(swapValue, policyType, swapMeta);

  return {
    policyId:      `pol-${swapId}-${policyType.toLowerCase()}`,
    swapId,
    buyerId,
    policyType,
    premium,
    riskFactor,
    coverageAmount,
    coveredEvents: [...POLICY_COVERAGE[policyType]],
    issuedAt:      new Date().toISOString(),
    status:        "ACTIVE",
  };
}

/**
 * File an insurance claim against a policy.
 *
 * @param {object} policy
 * @param {{ event, claimedAmount, evidence? }} claim
 * @returns {{ status, payout, reason, ... }}
 */
function fileClaim(policy, claim) {
  if (!policy || policy.status !== "ACTIVE")
    return { status: CLAIM_STATUSES.REJECTED, reason: "Policy is not active.", payout: 0 };

  const { event, claimedAmount, evidence = "" } = claim;

  if (!POLICY_COVERAGE[policy.policyType]?.has(event)) {
    return {
      status: CLAIM_STATUSES.REJECTED,
      reason: `Event '${event}' is not covered under ${policy.policyType} policy.`,
      payout: 0,
    };
  }

  if (typeof claimedAmount !== "number" || claimedAmount <= 0)
    return { status: CLAIM_STATUSES.REJECTED, reason: "claimedAmount must be positive.", payout: 0 };

  if (!evidence.trim())
    return { status: CLAIM_STATUSES.PENDING, reason: "Evidence required before approval.", payout: 0 };

  const capped     = Math.min(claimedAmount, policy.coverageAmount);
  const deductible = +(capped * DEDUCTIBLE_RATIO).toFixed(2);
  const payout     = +(capped - deductible).toFixed(2);

  return {
    status:          CLAIM_STATUSES.APPROVED,
    payout,
    deductible,
    claimedAmount,
    coverageApplied: capped,
    reason:          "Claim approved.",
  };
}

module.exports = {
  calculatePremium,
  issuePolicy,
  fileClaim,
  assessRiskFactor,
  POLICY_TYPES,
  COVERAGE_EVENTS,
  POLICY_COVERAGE,
  CLAIM_STATUSES,
  BASE_RATE,
  DEDUCTIBLE_RATIO,
};
