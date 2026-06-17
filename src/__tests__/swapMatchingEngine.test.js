const {
  scoreMatch,
  findMatchesForBuyer,
  batchMatch,
  WEIGHTS,
  MIN_MATCH_SCORE,
} = require("../matching/swapMatchingEngine");

const buyer = () => ({
  id: "buyer-1",
  assetType: "patent",
  price: 5000,
  maxPrice: 5000,
  minCondition: "good",
  categories: ["software.saas"],
  location: { country: "US", lat: 37.77, lon: -122.4 },
  maxDistanceKm: 200,
});

const seller = () => ({
  id: "seller-1",
  assetType: "patent",
  price: 4500,
  minPrice: 4000,
  condition: "excellent",
  categories: ["software.saas"],
  location: { country: "US", lat: 37.8, lon: -122.45 },
});

describe("scoreMatch", () => {
  test("perfect match scores 100", () => {
    const { score } = scoreMatch(buyer(), seller());
    expect(score).toBe(100);
  });

  test("asset type mismatch costs ASSET_TYPE weight", () => {
    const s = { ...seller(), assetType: "trademark" };
    const { score, breakdown } = scoreMatch(buyer(), s);
    expect(breakdown.assetType).toBe(0);
    expect(score).toBe(100 - WEIGHTS.ASSET_TYPE);
  });

  test("price gap near-miss gives partial score", () => {
    const b = { ...buyer(), maxPrice: 3800 };
    const { breakdown } = scoreMatch(b, seller());
    expect(breakdown.price).toBeGreaterThan(0);
    expect(breakdown.price).toBeLessThan(WEIGHTS.PRICE);
  });

  test("price gap > 20% gives zero price score", () => {
    const b = { ...buyer(), maxPrice: 2000 };
    const { breakdown } = scoreMatch(b, seller());
    expect(breakdown.price).toBe(0);
  });

  test("category parent match gives partial score", () => {
    const b = { ...buyer(), categories: ["software.analytics"] };
    const { breakdown } = scoreMatch(b, seller());
    expect(breakdown.category).toBeGreaterThan(0);
    expect(breakdown.category).toBeLessThan(WEIGHTS.CATEGORY);
  });

  test("condition below minimum penalised", () => {
    const s = { ...seller(), condition: "poor" };
    const { breakdown } = scoreMatch(buyer(), s);
    expect(breakdown.condition).toBeLessThan(WEIGHTS.CONDITION);
  });

  test("cross-country location scores 0 location points", () => {
    const s = { ...seller(), location: { country: "DE" } };
    const { breakdown } = scoreMatch(buyer(), s);
    expect(breakdown.location).toBe(0);
  });
});

describe("findMatchesForBuyer", () => {
  test("returns sellers sorted by score descending", () => {
    const sellers = [
      { ...seller(), id: "s1" },
      { ...seller(), id: "s2", assetType: "trademark" },
      { ...seller(), id: "s3", condition: "poor" },
    ];
    const results = findMatchesForBuyer(buyer(), sellers);
    expect(results[0].sellerId).toBe("s1");
    expect(results[0].score).toBeGreaterThanOrEqual(results[1]?.score ?? 0);
  });

  test("filters results below minScore threshold", () => {
    const sellers = [{ ...seller(), id: "s1", assetType: "trademark", minPrice: 9000 }];
    const results = findMatchesForBuyer(buyer(), sellers, { minScore: 80 });
    expect(results).toHaveLength(0);
  });

  test("respects maxResults cap", () => {
    const sellers = Array.from({ length: 30 }, (_, i) => ({ ...seller(), id: `s${i}` }));
    const results = findMatchesForBuyer(buyer(), sellers, { maxResults: 5 });
    expect(results).toHaveLength(5);
  });

  test("throws on invalid buyer", () => {
    expect(() => findMatchesForBuyer(null, [seller()])).toThrow(TypeError);
  });
});

describe("batchMatch", () => {
  test("matches multiple buyers against multiple sellers", () => {
    const buyers  = [buyer(), { ...buyer(), id: "buyer-2" }];
    const sellers = [seller(), { ...seller(), id: "seller-2" }];
    const result  = batchMatch(buyers, sellers);
    expect(result.totalBuyers).toBe(2);
    expect(result.results).toHaveLength(2);
  });

  test("throws on empty buyers array", () => {
    expect(() => batchMatch([], [seller()])).toThrow(TypeError);
  });

  test("throws on empty sellers array", () => {
    expect(() => batchMatch([buyer()], [])).toThrow(TypeError);
  });
});
