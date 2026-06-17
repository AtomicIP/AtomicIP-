const {
  matchOrders,
  MATCHING_ALGORITHMS,
  MAX_BATCH_SIZE,
} = require("../batch/batchMatcher");

const buy = (id, price, quantity, overrides = {}) => ({
  orderId: `buy-${id}`,
  party: `buyer-${id}`,
  price,
  quantity,
  timestamp: 1000,
  ...overrides,
});

const sell = (id, price, quantity, overrides = {}) => ({
  orderId: `sell-${id}`,
  party: `seller-${id}`,
  price,
  quantity,
  timestamp: 1000,
  ...overrides,
});

const NOW = Date.now();

describe("matchOrders — validation", () => {
  test("throws on empty buyOrders", () => {
    expect(() => matchOrders([], [sell("a", 10, 1)])).toThrow(TypeError);
  });

  test("throws on empty sellOrders", () => {
    expect(() => matchOrders([buy("a", 10, 1)], [])).toThrow(TypeError);
  });

  test("throws on buyOrders > MAX_BATCH_SIZE", () => {
    const buys = Array.from({ length: MAX_BATCH_SIZE + 1 }, (_, i) => buy(`b${i}`, 10, 1));
    expect(() => matchOrders(buys, [sell("a", 10, 1)])).toThrow(RangeError);
  });

  test("throws on sellOrders > MAX_BATCH_SIZE", () => {
    const sells = Array.from({ length: MAX_BATCH_SIZE + 1 }, (_, i) => sell(`s${i}`, 10, 1));
    expect(() => matchOrders([buy("a", 10, 1)], sells)).toThrow(RangeError);
  });

  test("throws on unknown algorithm", () => {
    expect(() =>
      matchOrders([buy("a", 10, 1)], [sell("b", 10, 1)], { algorithm: "unknown" })
    ).toThrow(TypeError);
  });

  test("records error for invalid buy order", () => {
    const result = matchOrders(
      [{ orderId: "bad", party: "x", price: -1, quantity: 1 }],
      [sell("a", 10, 1)]
    );
    expect(result.errors.length).toBeGreaterThan(0);
    expect(result.errors[0].side).toBe("buy");
  });

  test("records error for invalid sell order", () => {
    const result = matchOrders(
      [buy("a", 10, 1)],
      [{ orderId: "bad", party: "x", price: 10, quantity: 0 }]
    );
    expect(result.errors.length).toBeGreaterThan(0);
    expect(result.errors[0].side).toBe("sell");
  });

  test("records error when orderId is missing", () => {
    const result = matchOrders(
      [buy("a", 10, 1)],
      [{ party: "x", price: 10, quantity: 1 }]
    );
    expect(result.errors.length).toBe(1);
    expect(result.errors[0].error).toMatch(/orderId is required/);
  });

  test("records error when party is missing", () => {
    const result = matchOrders(
      [buy("a", 10, 1)],
      [{ orderId: "no-party", price: 10, quantity: 1 }]
    );
    expect(result.errors.length).toBe(1);
    expect(result.errors[0].error).toMatch(/party is required/);
  });

  test("records errors for both sides independently", () => {
    const result = matchOrders(
      [{ orderId: "b1", party: "x", price: 10, quantity: 0 }],
      [{ orderId: "s1", party: "y", price: -5, quantity: 1 }]
    );
    expect(result.errors.length).toBe(2);
  });
});

describe("matchOrders — single match", () => {
  test("matches one buyer with one seller at same price", () => {
    const result = matchOrders(
      [buy("a", 10, 5)],
      [sell("b", 10, 5)]
    );
    expect(result.matchedCount).toBe(1);
    expect(result.matches[0].quantity).toBe(5);
    expect(result.matches[0].price).toBe(10);
    expect(result.matches[0].total).toBe(50);
    expect(result.unmatchedBuyCount).toBe(0);
    expect(result.unmatchedSellCount).toBe(0);
  });

  test("matches when buyer offers higher price than seller asks", () => {
    const result = matchOrders(
      [buy("a", 20, 3)],
      [sell("b", 10, 3)]
    );
    expect(result.matchedCount).toBe(1);
    expect(result.matches[0].price).toBe(10);
    expect(result.matches[0].total).toBe(30);
  });

  test("does not match when buyer price is below seller price", () => {
    const result = matchOrders(
      [buy("a", 5, 3)],
      [sell("b", 10, 3)]
    );
    expect(result.matchedCount).toBe(0);
    expect(result.unmatchedBuyCount).toBe(1);
    expect(result.unmatchedSellCount).toBe(1);
  });

  test("respects ipId restriction when both specify it", () => {
    const result = matchOrders(
      [buy("a", 10, 1, { ipId: 1 })],
      [sell("b", 10, 1, { ipId: 2 })]
    );
    expect(result.matchedCount).toBe(0);
  });

  test("matches when only buyer specifies ipId", () => {
    const result = matchOrders(
      [buy("a", 10, 1, { ipId: 1 })],
      [sell("b", 10, 1)]
    );
    expect(result.matchedCount).toBe(1);
  });

  test("matches when only seller specifies ipId", () => {
    const result = matchOrders(
      [buy("a", 10, 1)],
      [sell("b", 10, 1, { ipId: 1 })]
    );
    expect(result.matchedCount).toBe(1);
  });

  test("sets correct batchSize", () => {
    const result = matchOrders(
      [buy("a", 10, 1)],
      [sell("b", 10, 1)]
    );
    expect(result.batchSize).toBe(2);
    expect(result.buyOrderCount).toBe(1);
    expect(result.sellOrderCount).toBe(1);
  });
});

describe("matchOrders — partial fills", () => {
  test("partial fill when buyer wants less than seller offers", () => {
    const result = matchOrders(
      [buy("a", 10, 3)],
      [sell("b", 10, 5)]
    );
    expect(result.matchedCount).toBe(1);
    expect(result.matches[0].quantity).toBe(3);
    expect(result.totalMatchedQuantity).toBe(3);
  });

  test("partial fill when buyer wants more than one seller offers", () => {
    const result = matchOrders(
      [buy("a", 10, 7)],
      [sell("b", 10, 5), sell("c", 10, 5)]
    );
    expect(result.matchedCount).toBe(2);
    expect(result.matches[0].quantity).toBe(5);
    expect(result.matches[1].quantity).toBe(2);
    expect(result.totalMatchedQuantity).toBe(7);
  });

  test("buyer remains partially unmatched", () => {
    const result = matchOrders(
      [buy("a", 10, 10)],
      [sell("b", 10, 4)]
    );
    expect(result.matchedCount).toBe(1);
    expect(result.totalMatchedQuantity).toBe(4);
    expect(result.unmatchedBuyCount).toBe(1);
    expect(result.unmatchedBuys[0].unmatchedQuantity).toBe(6);
  });

  test("seller remains partially unmatched", () => {
    const result = matchOrders(
      [buy("a", 10, 3)],
      [sell("b", 10, 10)]
    );
    expect(result.matchedCount).toBe(1);
    expect(result.totalMatchedQuantity).toBe(3);
    expect(result.unmatchedSellCount).toBe(1);
    expect(result.unmatchedSells[0].unmatchedQuantity).toBe(7);
  });
});

describe("matchOrders — price-time priority", () => {
  test("higher price buys match before lower price buys", () => {
    const result = matchOrders(
      [buy("low", 5, 5), buy("high", 15, 5)],
      [sell("a", 10, 5)]
    );
    expect(result.matchedCount).toBe(1);
    expect(result.matches[0].buyOrderId).toBe("buy-high");
  });

  test("lower price sells match before higher price sells", () => {
    const result = matchOrders(
      [buy("a", 15, 5)],
      [sell("high", 12, 5, { timestamp: 2000 }), sell("low", 8, 5, { timestamp: 1000 })]
    );
    expect(result.matchedCount).toBe(1);
    expect(result.matches[0].sellOrderId).toBe("sell-low");
    expect(result.matches[0].price).toBe(8);
  });

  test("earlier timestamp gets priority at same price", () => {
    const result = matchOrders(
      [buy("late", 10, 5, { timestamp: 2000 }), buy("early", 10, 5, { timestamp: 1000 })],
      [sell("a", 10, 5)]
    );
    expect(result.matchedCount).toBe(1);
    expect(result.matches[0].buyOrderId).toBe("buy-early");
  });
});

describe("matchOrders — multi-way matching", () => {
  test("multiple buyers matched to one seller", () => {
    const result = matchOrders(
      [buy("a", 10, 3), buy("b", 10, 4)],
      [sell("c", 10, 5)]
    );
    expect(result.matchedCount).toBe(2);
    expect(result.matches[0].buyOrderId).toBe("buy-a");
    expect(result.matches[0].quantity).toBe(3);
    expect(result.matches[1].buyOrderId).toBe("buy-b");
    expect(result.matches[1].quantity).toBe(2);
    expect(result.totalMatchedQuantity).toBe(5);
  });

  test("one buyer matched to multiple sellers", () => {
    const result = matchOrders(
      [buy("a", 10, 10)],
      [sell("b", 10, 4), sell("c", 10, 6)]
    );
    expect(result.matchedCount).toBe(2);
    expect(result.totalMatchedQuantity).toBe(10);
    expect(result.unmatchedBuyCount).toBe(0);
    expect(result.unmatchedSellCount).toBe(0);
  });

  test("full multi-way match: 3 buyers x 3 sellers", () => {
    const result = matchOrders(
      [buy("1", 15, 5), buy("2", 12, 3), buy("3", 10, 2)],
      [sell("a", 10, 4), sell("b", 11, 3), sell("c", 12, 3)]
    );
    expect(result.totalMatchedQuantity).toBe(8);
    expect(result.unmatchedBuyCount).toBe(1);
    expect(result.unmatchedSellCount).toBe(1);
    result.matches.forEach((m) => {
      expect(m.quantity).toBeGreaterThan(0);
    });
  });

  test("aggregate totals are correct", () => {
    const result = matchOrders(
      [buy("a", 10, 5)],
      [sell("b", 10, 5)]
    );
    expect(result.totalMatchedQuantity).toBe(5);
    expect(result.totalMatchedValue).toBe(50);
  });
});

describe("matchOrders — mixed batch with errors", () => {
  test("processes valid orders when some have errors", () => {
    const result = matchOrders(
      [buy("good", 10, 5), { orderId: "bad", party: "x", price: -1, quantity: 1 }],
      [sell("s1", 10, 5)]
    );
    expect(result.matchedCount).toBe(1);
    expect(result.unmatchedBuyCount).toBe(0);
    expect(result.errors.length).toBe(1);
  });

  test("no matching when all buy orders have errors", () => {
    const result = matchOrders(
      [{ orderId: "bad", party: "x", price: -1, quantity: 1 }],
      [sell("s1", 10, 5)]
    );
    expect(result.matchedCount).toBe(0);
    expect(result.errors.length).toBe(1);
  });

  test("no matching when all sell orders have errors", () => {
    const result = matchOrders(
      [buy("b1", 10, 5)],
      [{ orderId: "bad", party: "x", price: -1, quantity: 1 }]
    );
    expect(result.matchedCount).toBe(0);
    expect(result.errors.length).toBe(1);
  });
});
