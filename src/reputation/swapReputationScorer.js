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
 *
 * Persistence — Issue #878
 * ──────────────────────────────────────
 * `calculateReputationScore` / `batchCalculateReputation` above are pure
 * functions: given a history they return a score, nothing is written down.
 * That's fine for the API/contract layer's own request-scoped caching
 * (`api-server/src/cache.rs`, key prefix `reputation:`, TTL-based), but it
 * means there is no durable record a score ever existed once that cache
 * entry expires or the process restarts.
 *
 * This module fills that gap with a small `ReputationStore` interface —
 * `get(participantId)`, `set(participantId, record)`, `getAll()` — so the
 * scoring logic stays decoupled from *where* scores live:
 *
 *  - `MemoryReputationStore` — process-local Map, not durable. Default
 *    choice for tests and short-lived scripts.
 *  - `FileReputationStore`   — scores serialized to a JSON file on disk.
 *    Durable across process restarts; intended as the default backend for
 *    single-instance deployments/tooling that don't have a real DB handy.
 *
 * A production, multi-instance deployment should back this interface with
 * the API server's shared store instead (e.g. the Redis-backed cache in
 * `api-server/src/cache.rs`, or a proper DB table) by implementing the
 * same three methods — nothing above this layer needs to change.
 */

const fs   = require("fs");
const path = require("path");

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

/**
 * In-memory reputation store. Not durable — data is lost when the process
 * exits. Useful as the default in tests and short-lived scripts, and as a
 * reference implementation of the `ReputationStore` interface.
 */
class MemoryReputationStore {
  constructor() {
    this._records = new Map();
  }

  get(participantId) {
    return this._records.get(participantId) ?? null;
  }

  set(participantId, record) {
    this._records.set(participantId, record);
  }

  getAll() {
    return Array.from(this._records.values());
  }
}

/**
 * File-backed reputation store. Scores are serialized as JSON to disk, so
 * they survive process restarts — the store re-reads from disk on every
 * call rather than caching in memory, which keeps it correct if multiple
 * short-lived processes share the same file.
 */
class FileReputationStore {
  constructor(filePath) {
    if (!filePath) throw new TypeError("filePath is required.");
    this.filePath = filePath;
  }

  _readAll() {
    try {
      const raw = fs.readFileSync(this.filePath, "utf8");
      return JSON.parse(raw);
    } catch (err) {
      if (err.code === "ENOENT") return {};
      throw err;
    }
  }

  _writeAll(records) {
    const dir = path.dirname(this.filePath);
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(this.filePath, JSON.stringify(records, null, 2));
  }

  get(participantId) {
    const records = this._readAll();
    return records[participantId] ?? null;
  }

  set(participantId, record) {
    const records = this._readAll();
    records[participantId] = record;
    this._writeAll(records);
  }

  getAll() {
    return Object.values(this._readAll());
  }
}

/**
 * Calculate a participant's reputation score and persist it to `store`.
 *
 * @param {object} input - same shape as `calculateReputationScore`.
 * @param {{get, set, getAll}} store - a `ReputationStore` implementation.
 * @returns {object} the calculated result (same shape as
 *   `calculateReputationScore`), plus `updatedAt`.
 */
function persistReputationScore(input, store, nowMs = Date.now()) {
  if (!store || typeof store.set !== "function")
    throw new TypeError("store must implement the ReputationStore interface.");

  const result = calculateReputationScore(input, nowMs);
  const record = { ...result, updatedAt: new Date(nowMs).toISOString() };
  store.set(result.participantId, record);
  return record;
}

/**
 * Look up a previously persisted reputation score. Returns `null` if the
 * participant has no persisted record.
 */
function getPersistedReputationScore(participantId, store) {
  if (!store || typeof store.get !== "function")
    throw new TypeError("store must implement the ReputationStore interface.");
  return store.get(participantId);
}

module.exports = {
  calculateReputationScore,
  batchCalculateReputation,
  recencyWeight,
  scoreTier,
  STARTING_SCORE,
  MAX_SCORE,
  MIN_SCORE,
  MemoryReputationStore,
  FileReputationStore,
  persistReputationScore,
  getPersistedReputationScore,
};
