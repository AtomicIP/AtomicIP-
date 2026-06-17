const {
  calculateReputationScore,
  batchCalculateReputation,
  recencyWeight,
  scoreTier,
  STARTING_SCORE,
} = require("../reputation/swapReputationScorer");

const NOW = new Date("2024-06-01T00:00:00.000Z").getTime();
const daysAgo = (d) => new Date(NOW - d * 86_400_000).toISOString();

const makeHistory = (count, outcome = "completed", extras = {}) =>
  Array.from({ length: count }, (_, i) => ({
    outcome,
    role: "initiator",
    date: daysAgo(i * 5),
    rating: 5,
    disputed: false,
    ...extras,
  }));

describe("recencyWeight", () => {
  test("weight is 1.0 for today", () => {
    expect(recencyWeight(NOW, NOW)).toBeCloseTo(1.0, 4);
  });
  test("weight is ~0.5 at half-life", () => {
    expect(recencyWeight(NOW - 90 * 86_400_000, NOW)).toBeCloseTo(0.5, 1);
  });
  test("weight approaches 0 for very old events", () => {
    expect(recencyWeight(NOW - 3650 * 86_400_000, NOW)).toBeLessThan(0.01);
  });
});

describe("scoreTier", () => {
  test.each([
    [900, "platinum"], [750, "gold"], [600, "silver"],
    [450, "bronze"],   [200, "new"],
  ])("score %d → tier %s", (score, tier) => {
    expect(scoreTier(score)).toBe(tier);
  });
});

describe("calculateReputationScore", () => {
  test("throws on missing participantId", () => {
    expect(() => calculateReputationScore({ history: [] })).toThrow(TypeError);
  });

  test("new participant with no history starts at STARTING_SCORE", () => {
    const { score } = calculateReputationScore({ participantId: "p1", history: [] }, NOW);
    expect(score).toBe(STARTING_SCORE);
  });

  test("perfect history (completed, 5-star) scores high", () => {
    const history = makeHistory(20);
    const { score, tier } = calculateReputationScore(
      { participantId: "p1", history, accountCreatedAt: daysAgo(365) },
      NOW
    );
    expect(score).toBeGreaterThan(800);
    expect(["gold", "platinum"]).toContain(tier);
  });

  test("high dispute rate penalises score", () => {
    const history = makeHistory(20, "completed", { disputed: true });
    const { score: disputed } = calculateReputationScore({ participantId: "p1", history }, NOW);
    const { score: clean } = calculateReputationScore({ participantId: "p1", history: makeHistory(20) }, NOW);
    expect(disputed).toBeLessThan(clean);
  });

  test("cancellations penalise score", () => {
    const { score: cs } = calculateReputationScore({ participantId: "p1", history: makeHistory(10, "cancelled") }, NOW);
    const { score: co } = calculateReputationScore({ participantId: "p1", history: makeHistory(10) }, NOW);
    expect(cs).toBeLessThan(co);
  });

  test("dampened flag set for < 10 swaps", () => {
    const { dampened } = calculateReputationScore(
      { participantId: "p1", history: makeHistory(5) },
      NOW
    );
    expect(dampened).toBe(true);
  });

  test("score is clamped between 0 and 1000", () => {
    const bad = Array.from({ length: 50 }, (_, i) => ({
      outcome: "cancelled", role: "initiator", date: daysAgo(i),
      rating: 1, disputed: true,
    }));
    const { score } = calculateReputationScore({ participantId: "bad", history: bad }, NOW);
    expect(score).toBeGreaterThanOrEqual(0);
    expect(score).toBeLessThanOrEqual(1000);
  });
});

describe("batchCalculateReputation", () => {
  test("returns results sorted by score descending", () => {
    const inputs = [
      { participantId: "low", history: makeHistory(5, "cancelled") },
      { participantId: "high", history: makeHistory(20) },
    ];
    const results = batchCalculateReputation(inputs, NOW);
    expect(results[0].participantId).toBe("high");
  });

  test("throws on empty input", () => {
    expect(() => batchCalculateReputation([])).toThrow(TypeError);
  });
});
