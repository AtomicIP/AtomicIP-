/**
 * Swap Reputation Scoring — Issue #474
 * ──────────────────────────────────────
 * Scores buyers and sellers based on their swap history.
 */

const fs = require("fs");
const path = require("path");

const STARTING_SCORE = 500;
const MAX_SCORE = 1000;
const MIN_SCORE = 0;
const RECENCY_HALF_LIFE = 90;
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
  const disputes = history.filter((h) => h.disputed === true).length;
  const rate = disputes / completed;
  const penalty = -Math.round(Math.min(rate / 0.1, 1) * 150);
  return Object.is(penalty, -0) ? 0 : penalty;
}

function ratingScore(history, nowMs = Date.now()) {
  const rated = history.filter((h) => h.rating != null && h.rating >= 1 && h.rating <= 5);
  if (rated.length === 0) return 150;

  let weightedSum = 0;
  let totalWeight = 0;
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

function calculateReputationScore(input, nowMs = Date.now()) {
  const { participantId, history = [], accountCreatedAt } = input;
  if (!participantId) throw new TypeError("participantId is required.");
  if (!Array.isArray(history)) throw new TypeError("history must be an array.");

  const breakdown = {
    completion: completionScore(history),
    dispute: disputePenalty(history),
    rating: ratingScore(history, nowMs),
    tenure: tenureBonus(accountCreatedAt, nowMs),
    volume: volumeBonus(history),
    cancellation: cancellationPenalty(history, nowMs),
  };

  if (history.length === 0) {
    return { participantId, score: STARTING_SCORE, tier: "new", breakdown, swapCount: 0, dampened: true };
  }

  let raw = Object.values(breakdown).reduce((s, v) => s + v, 0);
  const dampened = history.length < MIN_SWAPS_FOR_FULL;
  if (!dampened) {
    raw = STARTING_SCORE + raw;
  } else {
    const weight = history.length / MIN_SWAPS_FOR_FULL;
    raw = STARTING_SCORE + (raw - STARTING_SCORE) * weight;
  }

  const score = Math.round(Math.min(MAX_SCORE, Math.max(MIN_SCORE, raw)));
  const tier = scoreTier(score);

  return { participantId, score, tier, breakdown, swapCount: history.length, dampened };
}

function batchCalculateReputation(inputs, nowMs = Date.now()) {
  if (!Array.isArray(inputs) || inputs.length === 0)
    throw new TypeError("inputs must be a non-empty array.");
  return inputs
    .map((input) => calculateReputationScore(input, nowMs))
    .sort((a, b) => b.score - a.score);
}

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

function persistReputationScore(input, store, nowMs = Date.now()) {
  if (!store || typeof store.set !== "function")
    throw new TypeError("store must implement the ReputationStore interface.");

  const result = calculateReputationScore(input, nowMs);
  const record = { ...result, updatedAt: new Date(nowMs).toISOString() };
  store.set(result.participantId, record);
  return record;
}

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
