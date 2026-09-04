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

/**
 * Assess a risk multiplier for a swap based on seller reputation, claim history,
 * transaction value, and asset category.
 *
 * @param {object} swapMeta
 * @param {number} [swapMeta.sellerReputationScore]  - 0–1000 reputation score
 * @param {number} [swapMeta.previousClaimsCount]    - number of past insurance claims
 * @param {number} [swapMeta.transactionValue]       - nominal swap value
 * @param {string} [swapMeta.assetCategory]          - e.g. "high_risk", "software"
 * @returns {number} risk factor ≥ 0.5 (applied as a premium multiplier)
 */
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

// ── Dispute-resolution → insurance payout link (#876) ──────────────────────────
//
// `swapInsurance.js` and the contract's dispute/arbitration flow
// (`arbitration_tests.rs`, `resolve_dispute`) are otherwise two separate
// systems: a dispute is resolved on-chain (or via `batchDisputeResolver.js`
// off-chain) with no awareness that the swap it concerns might be insured.
//
// Trigger condition: an insurance payout fires automatically when a dispute
// resolution leaves the insured buyer without full on-chain recovery of the
// swap amount. Concretely, given a `resolveOne()`/`resolveBatchDisputes()`
// result item for the same swap as the policy:
//
//   - REFUND   → the buyer already recovers the full amount via escrow;
//                no insurance payout is needed.
//   - RELEASE  → the buyer recovers nothing on-chain (the counterparty keeps
//                the full amount); the entire swap value is a shortfall.
//   - SPLIT    → the buyer recovers `initiatorAmount`; the remainder
//                (`counterpartyAmount`) is the shortfall the policy covers.
//   - ESCALATE → the dispute has not reached a final ruling; nothing is
//                triggered until it resolves to one of the above.
//
// The arbitration ruling itself stands in as the claim's evidence — a buyer
// who already went through on-chain/committee arbitration should not have to
// separately re-litigate the same facts to satisfy `fileClaim`'s evidence
// requirement.
//
// See docs/atomic-swap.md, "#876: Dispute-to-Insurance Payout Link".

const DISPUTE_RESOLVED_STATE = "RESOLVED";

/**
 * Determine whether a resolved swap dispute should trigger an automatic
 * insurance payout for the insured buyer, and if so, file (and adjudicate)
 * the resulting claim against the policy.
 *
 * This is the "explicit API call" side of the link: a dispute-resolution
 * consumer (an on-chain event listener, a webhook handler for
 * `resolve_dispute`, or a caller of `batchDisputeResolver.js`) calls this
 * with the policy and the matching resolution result.
 *
 * @param {InsurancePolicy} policy
 * @param {{ swapId: string, newState: string, resolutionType: string, initiatorAmount: number|null, counterpartyAmount: number|null }} disputeResolution
 *        A single result entry as produced by `resolveOne`/`resolveBatchDisputes`
 *        in `src/batch/batchDisputeResolver.js`.
 * @param {{ event?: string, evidence?: string }} [options]
 * @returns {{ triggered: boolean, reason?: string, shortfall?: number, claim?: object }}
 */
function evaluateDisputePayout(policy, disputeResolution, options = {}) {
  if (!policy || policy.status !== "ACTIVE")
    return { triggered: false, reason: "Policy is not active." };

  if (!disputeResolution || disputeResolution.swapId !== policy.swapId)
    return { triggered: false, reason: "Dispute resolution does not concern this policy's swap." };

  if (disputeResolution.newState !== DISPUTE_RESOLVED_STATE)
    return {
      triggered: false,
      reason: `Dispute is '${disputeResolution.newState}', not finally resolved.`,
    };

  const initiatorAmount    = disputeResolution.initiatorAmount    ?? 0;
  const counterpartyAmount = disputeResolution.counterpartyAmount ?? 0;
  const totalAmount        = initiatorAmount + counterpartyAmount;
  const shortfall          = +(totalAmount - initiatorAmount).toFixed(8);

  if (shortfall <= 0)
    return { triggered: false, reason: "Buyer received full recovery from dispute resolution; no payout needed." };

  const event    = options.event ?? COVERAGE_EVENTS.NON_DELIVERY;
  const evidence = options.evidence ??
    `Dispute for swap ${disputeResolution.swapId} resolved via ${disputeResolution.resolutionType}; ` +
    `the arbitration ruling itself is the evidence for this claim.`;

  const claim = fileClaim(policy, { event, claimedAmount: shortfall, evidence });

  return { triggered: true, shortfall, claim };
}

/**
 * Batch form of `evaluateDisputePayout` for a webhook/event-consumer style
 * integration: apply a set of dispute resolutions against the policies that
 * cover their swaps.
 *
 * @param {Array<InsurancePolicy>} policies
 * @param {Array<object>} disputeResolutions - results from `resolveBatchDisputes(...).results`
 * @param {{ event?: string, evidence?: string }} [options]
 * @returns {Array<{ swapId: string, triggered: boolean, reason?: string, shortfall?: number, claim?: object }>}
 */
function processDisputeResolutions(policies, disputeResolutions, options = {}) {
  if (!Array.isArray(policies)) throw new TypeError("policies must be an array.");
  if (!Array.isArray(disputeResolutions)) throw new TypeError("disputeResolutions must be an array.");

  const policyBySwapId = new Map(policies.map((p) => [p.swapId, p]));

  return disputeResolutions.map((resolution) => {
    const policy = policyBySwapId.get(resolution.swapId);
    if (!policy)
      return { swapId: resolution.swapId, triggered: false, reason: "No insurance policy found for this swap." };

    return { swapId: resolution.swapId, ...evaluateDisputePayout(policy, resolution, options) };
  });
}

module.exports = {
  calculatePremium,
  issuePolicy,
  fileClaim,
  assessRiskFactor,
  evaluateDisputePayout,
  processDisputeResolutions,
  POLICY_TYPES,
  COVERAGE_EVENTS,
  POLICY_COVERAGE,
  CLAIM_STATUSES,
  BASE_RATE,
  DEDUCTIBLE_RATIO,
};
