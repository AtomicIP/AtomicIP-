/**
 * Multi-Buyer-Seller Order Matching
 * ─────────────────────────────────
 * Matches multiple buy and sell orders using price-time priority.
 *
 * Matching rules:
 *  - Buyers and sellers are matched when buy.price >= sell.price
 *  - Orders are matched by price-time priority:
 *      Sell: lowest price first, then earliest timestamp
 *      Buy:  highest price first, then earliest timestamp
 *  - Partial fills are supported (creates one match entry per fill)
 *  - Orders may optionally specify an ipId to restrict matching
 *  - Unmatched orders are returned separately
 */

const MAX_BATCH_SIZE = 100;

const MATCHING_ALGORITHMS = Object.freeze({
  PRICE_TIME_PRIORITY: "priceTimePriority",
});

const DEFAULT_ALGORITHM = MATCHING_ALGORITHMS.PRICE_TIME_PRIORITY;

const VALID_SIDES = new Set(["buy", "sell"]);

function validateOrder(order, index, side) {
  if (!order || typeof order !== "object")
    throw new TypeError(`${side} order at index ${index} must be an object.`);
  if (!order.orderId)
    throw new TypeError(`${side} order at index ${index}: orderId is required.`);
  if (!order.party)
    throw new TypeError(`${side} order ${order.orderId}: party is required.`);
  if (typeof order.price !== "number" || order.price <= 0)
    throw new RangeError(`${side} order ${order.orderId}: price must be a positive number.`);
  if (typeof order.quantity !== "number" || order.quantity <= 0)
    throw new RangeError(`${side} order ${order.orderId}: quantity must be a positive number.`);
  if (order.timestamp != null && (typeof order.timestamp !== "number" || order.timestamp <= 0))
    throw new RangeError(`${side} order ${order.orderId}: timestamp must be a positive number if provided.`);
  if (order.ipId != null && typeof order.ipId !== "number")
    throw new TypeError(`${side} order ${order.orderId}: ipId must be a number if provided.`);
}

function validateBatch(buyOrders, sellOrders) {
  if (!Array.isArray(buyOrders) || buyOrders.length === 0)
    throw new TypeError("buyOrders must be a non-empty array.");
  if (!Array.isArray(sellOrders) || sellOrders.length === 0)
    throw new TypeError("sellOrders must be a non-empty array.");
  if (buyOrders.length > MAX_BATCH_SIZE)
    throw new RangeError(`buyOrders length ${buyOrders.length} exceeds maximum of ${MAX_BATCH_SIZE}.`);
  if (sellOrders.length > MAX_BATCH_SIZE)
    throw new RangeError(`sellOrders length ${sellOrders.length} exceeds maximum of ${MAX_BATCH_SIZE}.`);
}

function cloneAndSanitizeOrder(order) {
  return {
    orderId: order.orderId,
    party: order.party,
    ipId: order.ipId ?? null,
    price: order.price,
    quantity: order.quantity,
    timestamp: order.timestamp ?? 0,
  };
}

function priceTimePriorityMatch(buyOrders, sellOrders) {
  const buys = buyOrders.map(cloneAndSanitizeOrder).sort((a, b) => {
    if (b.price !== a.price) return b.price - a.price;
    return a.timestamp - b.timestamp;
  });

  const sells = sellOrders.map(cloneAndSanitizeOrder).sort((a, b) => {
    if (a.price !== b.price) return a.price - b.price;
    return a.timestamp - b.timestamp;
  });

  const matches = [];
  const unmatchedBuys = [];
  const sellAvailability = sells.map((s) => ({ ...s, remainingQuantity: s.quantity }));

  for (const buy of buys) {
    let buyRemaining = buy.quantity;
    let matched = false;

    for (let j = 0; j < sellAvailability.length && buyRemaining > 0; j++) {
      const sell = sellAvailability[j];
      if (sell.remainingQuantity <= 0) continue;
      if (buy.price < sell.price) continue;
      if (buy.ipId != null && sell.ipId != null && buy.ipId !== sell.ipId) continue;

      matched = true;
      const fillQuantity = Math.min(buyRemaining, sell.remainingQuantity);
      const fillPrice = sell.price;

      matches.push({
        buyOrderId: buy.orderId,
        sellOrderId: sell.orderId,
        buyer: buy.party,
        seller: sell.party,
        ipId: buy.ipId ?? sell.ipId,
        quantity: fillQuantity,
        price: fillPrice,
        total: +(fillQuantity * fillPrice).toFixed(8),
      });

      buyRemaining -= fillQuantity;
      sellAvailability[j].remainingQuantity -= fillQuantity;
    }

    if (!matched || buyRemaining > 0) {
      unmatchedBuys.push({
        ...buy,
        unmatchedQuantity: buyRemaining,
      });
    }
  }

  const unmatchedSells = sellAvailability
    .filter((s) => s.remainingQuantity > 0)
    .map((s) => ({
      orderId: s.orderId,
      party: s.party,
      ipId: s.ipId,
      price: s.price,
      quantity: s.quantity,
      timestamp: s.timestamp,
      unmatchedQuantity: s.remainingQuantity,
    }));

  return { matches, unmatchedBuys, unmatchedSells };
}

const ALGORITHM_MAP = {
  [MATCHING_ALGORITHMS.PRICE_TIME_PRIORITY]: priceTimePriorityMatch,
};

function matchOrders(buyOrders, sellOrders, options = {}) {
  validateBatch(buyOrders, sellOrders);

  const errors = [];

  buyOrders.forEach((o, i) => {
    try {
      validateOrder(o, i, "buy");
    } catch (err) {
      errors.push({ index: i, orderId: o?.orderId ?? null, side: "buy", error: err.message });
    }
  });

  sellOrders.forEach((o, i) => {
    try {
      validateOrder(o, i, "sell");
    } catch (err) {
      errors.push({ index: i, orderId: o?.orderId ?? null, side: "sell", error: err.message });
    }
  });

  const algorithm = options.algorithm ?? DEFAULT_ALGORITHM;
  if (!ALGORITHM_MAP[algorithm]) {
    throw new TypeError(`Unknown matching algorithm '${algorithm}'.`);
  }

  const validBuys = buyOrders.filter((_, i) => {
    return !errors.some((e) => e.side === "buy" && e.index === i);
  });
  const validSells = sellOrders.filter((_, i) => {
    return !errors.some((e) => e.side === "sell" && e.index === i);
  });

  let matches = [];
  let unmatchedBuys = [];
  let unmatchedSells = [];

  if (validBuys.length > 0 && validSells.length > 0) {
    const result = ALGORITHM_MAP[algorithm](validBuys, validSells);
    matches = result.matches;
    unmatchedBuys = result.unmatchedBuys;
    unmatchedSells = result.unmatchedSells;
  }

  const totalMatchedQuantity = matches.reduce((s, m) => s + m.quantity, 0);
  const totalMatchedValue = matches.reduce((s, m) => s + m.total, 0);

  return {
    batchSize: buyOrders.length + sellOrders.length,
    buyOrderCount: buyOrders.length,
    sellOrderCount: sellOrders.length,
    matchedCount: matches.length,
    unmatchedBuyCount: unmatchedBuys.length,
    unmatchedSellCount: unmatchedSells.length,
    totalMatchedQuantity: +totalMatchedQuantity.toFixed(8),
    totalMatchedValue: +totalMatchedValue.toFixed(8),
    matches,
    unmatchedBuys,
    unmatchedSells,
    errors,
  };
}

module.exports = {
  matchOrders,
  MATCHING_ALGORITHMS,
  MAX_BATCH_SIZE,
};
