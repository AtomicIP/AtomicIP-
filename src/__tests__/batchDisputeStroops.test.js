/**
 * Issue #1261: dispute splits must be exact in integer stroops (1e-7 XLM is
 * 1 stroop; amounts here are carried at 8 decimal places). Shares must always
 * sum to the disputed total.
 */
const fc = require('fast-check');
const { resolveBatchDisputes, RESOLUTION_TYPES } = require('../batch/batchDisputeResolver');

const toStroops = (n) => BigInt(Math.round(n * 1e8));

function openDispute(swapId, amount) {
  return { swapId, state: 'OPEN', amount, initiator: 'alice', counterparty: 'bob' };
}

describe('batch dispute split arithmetic (#1261)', () => {
  test('split shares sum exactly to the total for a 3-stroop dispute', () => {
    const { results, errors } = resolveBatchDisputes(
      [openDispute('s1', 0.00000003)],
      [{ type: RESOLUTION_TYPES.SPLIT, splitRatio: 0.5 }]
    );
    expect(errors).toEqual([]);
    const r = results[0];
    expect(toStroops(r.initiatorAmount) + toStroops(r.counterpartyAmount))
      .toBe(toStroops(0.00000003));
  });

  test('split shares sum exactly to the total for an uneven ratio', () => {
    const { results } = resolveBatchDisputes(
      [openDispute('s2', 1.23456789)],
      [{ type: RESOLUTION_TYPES.SPLIT, splitRatio: 0.333 }]
    );
    const r = results[0];
    expect(toStroops(r.initiatorAmount) + toStroops(r.counterpartyAmount))
      .toBe(toStroops(1.23456789));
  });

  test('property: split shares always sum to the disputed amount in stroops', () => {
    fc.assert(
      fc.property(
        fc.integer({ min: 1, max: 100_000_000_000 }),
        fc.integer({ min: 1, max: 99 }),
        (amountStroops, pct) => {
          const amount = Number(amountStroops) / 1e8;
          const { results, errors } = resolveBatchDisputes(
            [openDispute('p', amount)],
            [{ type: RESOLUTION_TYPES.SPLIT, splitRatio: pct / 100 }]
          );
          if (errors.length) return false;
          const r = results[0];
          return toStroops(r.initiatorAmount) + toStroops(r.counterpartyAmount) === amountStroops;
        }
      ),
      { numRuns: 300 }
    );
  });

  test('batch totalSplit equals the sum of split totals in stroops', () => {
    const disputes = [openDispute('a', 0.1), openDispute('b', 0.2), openDispute('c', 0.3)];
    const resolutions = disputes.map(() => ({ type: RESOLUTION_TYPES.SPLIT, splitRatio: 0.7 }));
    const summary = resolveBatchDisputes(disputes, resolutions);
    expect(toStroops(summary.totalSplit)).toBe(toStroops(0.1) + toStroops(0.2) + toStroops(0.3));
  });
});
