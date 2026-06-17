/**
 * Swap Reputation Scoring — Issue #474
 * ──────────────────────────────────────
 * Scores buyers and sellers based on their swap history.
 *
 * Score range: 0–1000 (higher = better reputation)
 * Starting score for new participants: 500
 *
 * Scoring factors:
 *  - Completion rate      (swaps completed / initiated)
 *  - Dispute rate         (disputes / completed)
 *  - Average rating       (1–5 stars, weighted by recency)
 *  - Tenure bonus         (account age in days)
 *  - Volume bonus         (total swap count)
 *  - Cancellation penalty (cancelled swaps)
 */

const STARTING_SCORE     = 500;
const MAX_SCORE          = 1000;
const MIN_SCORE          = 0;
const RECENCY_HALF_LIFE  = 90;
const MIN_SWAPS_FOR_FULL = 10;

function recencyWeight(eventDateMs, nowMs = Date.now()) {
  const agedays = (nowMs - eventDateMs) / 86_400_000;
  return Math.exp((-Math.LN2 * agedays) / RECENCY_HALF_LIFE);
}

function completionScore(history) {
  const initiated = history.filter((h) => h.role === "initiator").length;
  if (initiated === 0) return 100;
  const completed = history.filter((h) => h.role === "initiator" && h.outcome === "completed").length;
  return Math.round((completed / initiated) * 200);
}

function disputePenalty(history) {
  const completed = history.filter((h) => h.outcome === "completed").length;
  if (completed === 0) return 0;
  const disputes  = history.filter((h) => h.disputed === true).length;
  const rate      = disputes / completed;
  return -Math.round(Math.min(rate / 0.1, 1) * 150);
}

function ratingScore(history, nowMs = Date.now()) {
  const rated = history.filter((h) => h.rating != null && h.rating >= 1 && h.rating <= 5);
  if (rated.length === 0) return 150;

  let weightedSum = 0, totalWeight = 0;
  for (const h of rated) {
    const w = recencyWeight(new Date(h.date).getTime(), nowMs);
    weightedSum += h.rating * w;
    totalWeight += w;
  }
  const avg = totalWeight > 0 ? weightedSum / totalWeight : 3;
  return Math.round(((avg - 1) / 4) * 300);
}

function tenureBonus(accountCreatedAt, nowMs = Date.now()) {
  if (!accountCreatedAt) return 0;
  const agedays = (nowMs - new Date(accountCreatedAt).getTime()) / 86_400_000;
  return Math.round(Math.min(Math.log1p(agedays) / Math.log1p(730), 1) * 100);
}

function volumeBonus(history) {
  const count = history.length;
  return Math.round(Math.min(Math.sqrt(count) / Math.sqrt(200), 1) * 100);
}

function cancellationPenalty(history, nowMs = Date.now()) {
  const cancellations = history.filter((h) => h.outcome === "cancelled");
  if (cancellations.length === 0) return 0;
  const weightedCancels = cancellations.reduce(
    (s, h) => s + recencyWeight(new Date(h.date).getTime(), nowMs),
    0
  );
  return -Math.round(Math.min(weightedCancels / 5, 1) * 150);
}

function scoreTier(score) {
  if (score >= 850) return "platinum";
  if (score >= 700) return "gold";
  if (score >= 550) return "silver";
  if (score >= 400) return "bronze";
  return "new";
}

/**
 * Calculate reputation score for a participant.
 *
 * @param {object} input - { participantId, history, accountCreatedAt? }
 * @returns {{ participantId, score, tier, breakdown, swapCount, dampened }}
 */
function calculateReputationScore(input, nowMs = Date.now()) {
  const { participantId, history = [], accountCreatedAt } = input;
  if (!participantId) throw new TypeError("participantId is required.");
  if (!Array.isArray(history)) throw new TypeError("history must be an array.");

  const breakdown = {
    completion:   completionScore(history),
    dispute:      disputePenalty(history),
    rating:       ratingScore(history, nowMs),
    tenure:       tenureBonus(accountCreatedAt, nowMs),
    volume:       volumeBonus(history),
    cancellation: cancellationPenalty(history, nowMs),
  };

  let raw = Object.values(breakdown).reduce((s, v) => s + v, 0);

  const dampened = history.length < MIN_SWAPS_FOR_FULL;
  if (dampened) {
    const weight = history.length / MIN_SWAPS_FOR_FULL;
    raw = STARTING_SCORE + (raw - STARTING_SCORE) * weight;
  }

  const score = Math.round(Math.min(MAX_SCORE, Math.max(MIN_SCORE, raw)));
  const tier  = scoreTier(score);

  return { participantId, score, tier, breakdown, swapCount: history.length, dampened };
}

/**
 * Batch score multiple participants, sorted by score descending.
 */
function batchCalculateReputation(inputs, nowMs = Date.now()) {
  if (!Array.isArray(inputs) || inputs.length === 0)
    throw new TypeError("inputs must be a non-empty array.");
  return inputs
    .map((input) => calculateReputationScore(input, nowMs))
    .sort((a, b) => b.score - a.score);
}

module.exports = {
  calculateReputationScore,
  batchCalculateReputation,
  recencyWeight,
  scoreTier,
  STARTING_SCORE,
  MAX_SCORE,
  MIN_SCORE,
};
