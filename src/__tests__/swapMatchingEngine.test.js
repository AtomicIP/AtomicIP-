const {
  scoreMatch,
  findMatchesForBuyer,
  batchMatch,
  reconcileMatchOnChain,
  reconcileMatchBeforeSubmission,
  isMatchStale,
  WEIGHTS,
  MIN_MATCH_SCORE,
  MATCH_STALENESS_WINDOW_MS,
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

// ── #877: reconciliation between matches and on-chain swap state ──────────────

describe("findMatchesForBuyer — match timestamps", () => {
  test("each match is stamped with matchedAt for later staleness checks", () => {
    const now = 1_700_000_000_000;
    const results = findMatchesForBuyer(buyer(), [seller()], { now });
    expect(results[0].matchedAt).toBe(now);
  });
});

describe("isMatchStale", () => {
  test("a fresh match is not stale", () => {
    const now = 1_700_000_000_000;
    const match = { matchedAt: now };
    expect(isMatchStale(match, now + 1000)).toBe(false);
  });

  test("a match older than the staleness window is stale", () => {
    const now = 1_700_000_000_000;
    const match = { matchedAt: now };
    expect(isMatchStale(match, now + MATCH_STALENESS_WINDOW_MS + 1)).toBe(true);
  });

  test("a match exactly at the staleness window boundary is not yet stale", () => {
    const now = 1_700_000_000_000;
    const match = { matchedAt: now };
    expect(isMatchStale(match, now + MATCH_STALENESS_WINDOW_MS)).toBe(false);
  });

  test("a match with no matchedAt cannot be judged stale by time", () => {
    expect(isMatchStale({})).toBe(false);
  });

  test("respects a custom window", () => {
    const now = 1_700_000_000_000;
    const match = { matchedAt: now };
    expect(isMatchStale(match, now + 5000, 1000)).toBe(true);
  });
});

describe("reconcileMatchOnChain", () => {
  const freshMatch = () => findMatchesForBuyer(buyer(), [seller()])[0];

  test("a match against a still-PENDING on-chain swap is matchable", async () => {
    const getSwapState = jest.fn().mockResolvedValue("PENDING");
    const result = await reconcileMatchOnChain(freshMatch(), getSwapState);
    expect(result.matchable).toBe(true);
    expect(result.onChainState).toBe("PENDING");
    expect(getSwapState).toHaveBeenCalledWith("seller-1");
  });

  test("a match against an on-chain CANCELLED swap is no longer matchable", async () => {
    const getSwapState = jest.fn().mockReturnValue("CANCELLED");
    const result = await reconcileMatchOnChain(freshMatch(), getSwapState);
    expect(result.matchable).toBe(false);
    expect(result.reason).toMatch(/CANCELLED.*no longer be matched/);
  });

  test("supports a synchronous getSwapState as well as an async one", async () => {
    const result = await reconcileMatchOnChain(freshMatch(), () => "ACTIVE");
    expect(result.matchable).toBe(true);
  });

  test("prefers sellerListing.swapId over sellerId when present", async () => {
    const match = { ...freshMatch() };
    match.sellerListing = { ...match.sellerListing, swapId: "onchain-swap-42" };
    const getSwapState = jest.fn().mockResolvedValue("PENDING");
    await reconcileMatchOnChain(match, getSwapState);
    expect(getSwapState).toHaveBeenCalledWith("onchain-swap-42");
  });

  test("throws if getSwapState is not a function", async () => {
    await expect(reconcileMatchOnChain(freshMatch(), null)).rejects.toThrow(TypeError);
  });
});

describe("reconcileMatchBeforeSubmission", () => {
  test("a fresh, still-matchable match is submittable", async () => {
    const now = 1_700_000_000_000;
    const match = findMatchesForBuyer(buyer(), [seller()], { now })[0];
    const result = await reconcileMatchBeforeSubmission(match, () => "PENDING", { now: now + 5 });
    expect(result.submittable).toBe(true);
  });

  test("a match becoming stale between matching and submission is rejected without an on-chain call", async () => {
    const now = 1_700_000_000_000;
    const match = findMatchesForBuyer(buyer(), [seller()], { now })[0];
    const getSwapState = jest.fn().mockResolvedValue("PENDING");

    const result = await reconcileMatchBeforeSubmission(
      match,
      getSwapState,
      { now: now + MATCH_STALENESS_WINDOW_MS + 1 }
    );

    expect(result.submittable).toBe(false);
    expect(result.reason).toMatch(/staleness window/);
    expect(getSwapState).not.toHaveBeenCalled();
  });

  test("a fresh match whose swap was cancelled on-chain in the interim is rejected", async () => {
    const now = 1_700_000_000_000;
    const match = findMatchesForBuyer(buyer(), [seller()], { now })[0];

    const result = await reconcileMatchBeforeSubmission(
      match,
      () => "CANCELLED",
      { now: now + 100 }
    );

    expect(result.submittable).toBe(false);
    expect(result.onChainState).toBe("CANCELLED");
  });

  test("honors a custom staleWindowMs", async () => {
    const now = 1_700_000_000_000;
    const match = findMatchesForBuyer(buyer(), [seller()], { now })[0];
    const getSwapState = jest.fn().mockResolvedValue("PENDING");

    const result = await reconcileMatchBeforeSubmission(
      match,
      getSwapState,
      { now: now + 2000, staleWindowMs: 1000 }
    );

    expect(result.submittable).toBe(false);
    expect(getSwapState).not.toHaveBeenCalled();
  });
});
