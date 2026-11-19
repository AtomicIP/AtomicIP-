/**
 * Issue #1262: match totals must be exact integer-stroop products and must
 * equal the sum of the individual fills.
 */
const fc = require('fast-check');
const { matchOrders } = require('../batch/batchMatcher');

const toStroops = (n) => BigInt(Math.round(n * 1e8));

describe('batch matcher totals (#1262)', () => {
  test('a single fill total equals quantity × price in stroops', () => {
    const { matches } = matchOrders(
      [{ orderId: 'b1', party: 'buyer', price: 0.3, quantity: 0.7, timestamp: 1 }],
      [{ orderId: 's1', party: 'seller', price: 0.2, quantity: 1, timestamp: 1 }]
    );
    expect(matches).toHaveLength(1);
    const m = matches[0];
    expect(toStroops(m.total)).toBe((toStroops(m.quantity) * toStroops(m.price)) / 100000000n);
  });

  test('sum of fill totals equals the sum of exact per-fill products', () => {
    const buys = [
      { orderId: 'b1', party: 'b', price: 0.37, quantity: 1.11, timestamp: 1 },
      { orderId: 'b2', party: 'b', price: 0.41, quantity: 2.22, timestamp: 2 },
    ];
    const sells = [
      { orderId: 's1', party: 's', price: 0.31, quantity: 0.9, timestamp: 1 },
      { orderId: 's2', party: 's', price: 0.33, quantity: 2.5, timestamp: 2 },
    ];
    const { matches } = matchOrders(buys, sells);
    const sumTotals = matches.reduce((acc, m) => acc + toStroops(m.total), 0n);
    const sumExact = matches.reduce(
      (acc, m) => acc + (toStroops(m.quantity) * toStroops(m.price)) / 100000000n,
      0n
    );
    expect(sumTotals).toBe(sumExact);
  });

  test('property: fill totals over random 8-dp orders never drift from exact stroops', () => {
    const order = (id, party, s) =>
      fc.record({
        orderId: fc.constant(id),
        party: fc.constant(party),
        price: fc.integer({ min: 1, max: 1_000_000_000 }).map((n) => n / 1e8),
        quantity: fc.integer({ min: 1, max: 1_000_000_000 }).map((n) => n / 1e8),
        timestamp: fc.constant(s),
      });

    fc.assert(
      fc.property(
        fc.array(order('b', 'buyer', 1), { minLength: 1, maxLength: 5 }),
        fc.array(order('s', 'seller', 1), { minLength: 1, maxLength: 5 }),
        (buys, sells) => {
          const { matches } = matchOrders(
            buys.map((o, i) => ({ ...o, orderId: `b${i}` })),
            sells.map((o, i) => ({ ...o, orderId: `s${i}` }))
          );
          const sumTotals = matches.reduce((acc, m) => acc + toStroops(m.total), 0n);
          const sumExact = matches.reduce(
            (acc, m) => acc + (toStroops(m.quantity) * toStroops(m.price)) / 100000000n,
            0n
          );
          return sumTotals === sumExact;
        }
      ),
      { numRuns: 300 }
    );
  });
});
