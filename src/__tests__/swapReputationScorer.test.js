const fs   = require("fs");
const os   = require("os");
const path = require("path");

const {
  calculateReputationScore,
  batchCalculateReputation,
  recencyWeight,
  scoreTier,
  STARTING_SCORE,
  MemoryReputationStore,
  FileReputationStore,
  persistReputationScore,
  getPersistedReputationScore,
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

describe("persistence", () => {
  describe("MemoryReputationStore", () => {
    test("persists and retrieves a score within the same process", () => {
      const store = new MemoryReputationStore();
      const history = makeHistory(20);
      persistReputationScore({ participantId: "p1", history }, store, NOW);

      const persisted = getPersistedReputationScore("p1", store);
      expect(persisted).not.toBeNull();
      expect(persisted.participantId).toBe("p1");
      expect(persisted.updatedAt).toBe(new Date(NOW).toISOString());
    });

    test("returns null for an unknown participant", () => {
      const store = new MemoryReputationStore();
      expect(getPersistedReputationScore("nobody", store)).toBeNull();
    });
  });

  describe("FileReputationStore", () => {
    let filePath;

    beforeEach(() => {
      filePath = path.join(
        fs.mkdtempSync(path.join(os.tmpdir(), "reputation-store-")),
        "scores.json"
      );
    });

    afterEach(() => {
      fs.rmSync(path.dirname(filePath), { recursive: true, force: true });
    });

    test("persists a score across process restarts", () => {
      const history = makeHistory(20);

      // "Before restart": compute and persist with one store instance.
      const storeBefore = new FileReputationStore(filePath);
      const written = persistReputationScore({ participantId: "p1", history }, storeBefore, NOW);

      // "After restart": a brand-new store instance pointed at the same
      // file, with no shared in-memory state, must still see the score.
      const storeAfter = new FileReputationStore(filePath);
      const reloaded = getPersistedReputationScore("p1", storeAfter);

      expect(reloaded).toEqual(written);
      expect(reloaded.score).toBe(written.score);
    });

    test("getAll returns every persisted participant after reload", () => {
      const storeBefore = new FileReputationStore(filePath);
      persistReputationScore({ participantId: "p1", history: makeHistory(20) }, storeBefore, NOW);
      persistReputationScore({ participantId: "p2", history: makeHistory(3, "cancelled") }, storeBefore, NOW);

      const storeAfter = new FileReputationStore(filePath);
      const ids = storeAfter.getAll().map((r) => r.participantId).sort();
      expect(ids).toEqual(["p1", "p2"]);
    });

    test("returns null for an unknown participant without creating the file early", () => {
      const store = new FileReputationStore(filePath);
      expect(getPersistedReputationScore("nobody", store)).toBeNull();
      expect(fs.existsSync(filePath)).toBe(false);
    });
  });

  test("persistReputationScore throws without a valid store", () => {
    expect(() =>
      persistReputationScore({ participantId: "p1", history: [] }, {})
    ).toThrow(TypeError);
  });

  test("getPersistedReputationScore throws without a valid store", () => {
    expect(() => getPersistedReputationScore("p1", {})).toThrow(TypeError);
  });
});
