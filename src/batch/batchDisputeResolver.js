/**
 * Swap Batch Dispute Resolution
 * ─────────────────────────────
 * Resolves disputes for multiple swaps in a single batch operation.
 *
 * Dispute lifecycle:
 *   OPEN → (RESOLVED | REJECTED | ESCALATED)
 *
 * Resolution types:
 *   - REFUND:   Return funds to initiator
 *   - RELEASE:  Release funds to counterparty
 *   - SPLIT:    Split funds by a configured ratio
 *   - ESCALATE: Hand off to human arbitration
 */

const DISPUTE_STATES = Object.freeze({
  OPEN:      "OPEN",
  RESOLVED:  "RESOLVED",
  REJECTED:  "REJECTED",
  ESCALATED: "ESCALATED",
});

const RESOLUTION_TYPES = Object.freeze({
  REFUND:   "REFUND",
  RELEASE:  "RELEASE",
  SPLIT:    "SPLIT",
  ESCALATE: "ESCALATE",
});

const MAX_BATCH_SIZE       = 50;
const DEFAULT_SPLIT_RATIO  = 0.5; // 50/50 if not specified
const VALID_SPLIT_RANGE    = [0.01, 0.99];

// ── Validators ────────────────────────────────────────────────────────────────

function validateDispute(dispute, index) {
  if (!dispute || typeof dispute !== "object")
    throw new TypeError(`Dispute at index ${index} must be an object.`);
  if (!dispute.swapId)
    throw new TypeError(`Dispute at index ${index}: swapId is required.`);
  if (!Object.values(DISPUTE_STATES).includes(dispute.state))
    throw new TypeError(`Dispute at index ${index}: invalid state '${dispute.state}'.`);
  if (dispute.state !== DISPUTE_STATES.OPEN)
    throw new Error(`Dispute ${dispute.swapId}: only OPEN disputes can be resolved (got '${dispute.state}').`);
  if (typeof dispute.amount !== "number" || dispute.amount <= 0)
    throw new RangeError(`Dispute ${dispute.swapId}: amount must be a positive number.`);
}

function validateResolution(resolution, dispute) {
  if (!resolution || typeof resolution !== "object")
    throw new TypeError(`Resolution for swap ${dispute.swapId} must be an object.`);
  if (!Object.values(RESOLUTION_TYPES).includes(resolution.type))
    throw new TypeError(`Resolution for swap ${dispute.swapId}: invalid type '${resolution.type}'.`);
  if (resolution.type === RESOLUTION_TYPES.SPLIT) {
    const ratio = resolution.splitRatio ?? DEFAULT_SPLIT_RATIO;
    if (ratio < VALID_SPLIT_RANGE[0] || ratio > VALID_SPLIT_RANGE[1])
      throw new RangeError(
        `Dispute ${dispute.swapId}: splitRatio must be between ${VALID_SPLIT_RANGE[0]} and ${VALID_SPLIT_RANGE[1]}.`
      );
  }
}

// ── Resolution logic ──────────────────────────────────────────────────────────

function resolveOne(dispute, resolution) {
  const { type, splitRatio = DEFAULT_SPLIT_RATIO, reason = "" } = resolution;

  switch (type) {
    case RESOLUTION_TYPES.REFUND:
      return {
        swapId:           dispute.swapId,
        originalState:    dispute.state,
        newState:         DISPUTE_STATES.RESOLVED,
        resolutionType:   type,
        initiatorAmount:  dispute.amount,
        counterpartyAmount: 0,
        reason,
        resolvedAt:       new Date().toISOString(),
      };

    case RESOLUTION_TYPES.RELEASE:
      return {
        swapId:             dispute.swapId,
        originalState:      dispute.state,
        newState:           DISPUTE_STATES.RESOLVED,
        resolutionType:     type,
        initiatorAmount:    0,
        counterpartyAmount: dispute.amount,
        reason,
        resolvedAt:         new Date().toISOString(),
      };

    case RESOLUTION_TYPES.SPLIT: {
      const initiatorShare    = +(dispute.amount * splitRatio).toFixed(8);
      const counterpartyShare = +(dispute.amount - initiatorShare).toFixed(8);
      return {
        swapId:             dispute.swapId,
        originalState:      dispute.state,
        newState:           DISPUTE_STATES.RESOLVED,
        resolutionType:     type,
        splitRatio,
        initiatorAmount:    initiatorShare,
        counterpartyAmount: counterpartyShare,
        reason,
        resolvedAt:         new Date().toISOString(),
      };
    }

    case RESOLUTION_TYPES.ESCALATE:
      return {
        swapId:           dispute.swapId,
        originalState:    dispute.state,
        newState:         DISPUTE_STATES.ESCALATED,
        resolutionType:   type,
        initiatorAmount:  null,
        counterpartyAmount: null,
        reason,
        resolvedAt:       new Date().toISOString(),
      };

    default:
      throw new Error(`Unknown resolution type: ${type}`);
  }
}

// ── Batch entry point ─────────────────────────────────────────────────────────

/**
 * Resolve disputes for a batch of swaps.
 *
 * @param {Array<DisputeEntry>} disputes
 * @param {Array<ResolutionEntry>} resolutions  - parallel array, same order as disputes
 * @returns {BatchDisputeResult}
 *
 * @typedef {{ swapId: string, state: string, amount: number }} DisputeEntry
 * @typedef {{ type: string, splitRatio?: number, reason?: string }} ResolutionEntry
 *
 * @typedef {Object} BatchDisputeResult
 * @property {number} batchSize
 * @property {number} resolvedCount
 * @property {number} escalatedCount
 * @property {number} failedCount
 * @property {number} totalRefunded
 * @property {number} totalReleased
 * @property {number} totalSplit
 * @property {Array}  results
 * @property {Array}  errors
 */
function resolveBatchDisputes(disputes, resolutions) {
  if (!Array.isArray(disputes) || disputes.length === 0)
    throw new TypeError("disputes must be a non-empty array.");
  if (!Array.isArray(resolutions) || resolutions.length !== disputes.length)
    throw new TypeError("resolutions must be an array of the same length as disputes.");
  if (disputes.length > MAX_BATCH_SIZE)
    throw new RangeError(`Batch size ${disputes.length} exceeds maximum of ${MAX_BATCH_SIZE}.`);

  const results = [];
  const errors  = [];

  for (let i = 0; i < disputes.length; i++) {
    try {
      validateDispute(disputes[i], i);
      validateResolution(resolutions[i], disputes[i]);
      results.push(resolveOne(disputes[i], resolutions[i]));
    } catch (err) {
      errors.push({ index: i, swapId: disputes[i]?.swapId ?? null, error: err.message });
    }
  }

  const resolved   = results.filter((r) => r.newState === DISPUTE_STATES.RESOLVED);
  const escalated  = results.filter((r) => r.newState === DISPUTE_STATES.ESCALATED);

  const totalRefunded  = resolved.filter((r) => r.resolutionType === RESOLUTION_TYPES.REFUND)
                                  .reduce((s, r) => s + r.initiatorAmount, 0);
  const totalReleased  = resolved.filter((r) => r.resolutionType === RESOLUTION_TYPES.RELEASE)
                                  .reduce((s, r) => s + r.counterpartyAmount, 0);
  const totalSplit     = resolved.filter((r) => r.resolutionType === RESOLUTION_TYPES.SPLIT)
                                  .reduce((s, r) => s + r.initiatorAmount + r.counterpartyAmount, 0);

  return {
    batchSize:       disputes.length,
    resolvedCount:   resolved.length,
    escalatedCount:  escalated.length,
    failedCount:     errors.length,
    totalRefunded:   +totalRefunded.toFixed(8),
    totalReleased:   +totalReleased.toFixed(8),
    totalSplit:      +totalSplit.toFixed(8),
    results,
    errors,
  };
}

module.exports = {
  resolveBatchDisputes,
  DISPUTE_STATES,
  RESOLUTION_TYPES,
  MAX_BATCH_SIZE,
};
