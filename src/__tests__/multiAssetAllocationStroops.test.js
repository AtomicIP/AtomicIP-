/**
 * Issue #1263: per-asset allocations must sum exactly to the bundle total,
 * measured in integer stroops, including bundles with uneven prices.
 */
const fc = require('fast-check');
const { createMultiAssetSwap, getAssetAllocation } = require('../swap/multiAssetSwap');

const toStroops = (n) => BigInt(Math.round(n * 1e8));

function allocationSum(swap) {
  return getAssetAllocation(swap).reduce((acc, a) => acc + toStroops(a.allocatedPrice), 0n);
}

function swapWith(assets, totalPrice) {
  return createMultiAssetSwap({
    swapId: 'bundle',
    seller: 'seller',
    buyer: 'buyer',
    assets,
    totalPrice,
  });
}

describe('multi-asset allocation (#1263)', () => {
  test('three equal assets sharing an odd total still sum to the total', () => {
    const swap = swapWith(
      [
        { assetId: 'a', price: 1 },
        { assetId: 'b', price: 1 },
        { assetId: 'c', price: 1 },
      ],
      1
    );
    expect(allocationSum(swap)).toBe(toStroops(1));
  });

  test('uneven prices allocate to the exact bundle total', () => {
    const swap = swapWith(
      [
        { assetId: 'p1', price: 0.1 },
        { assetId: 'p2', price: 0.25 },
        { assetId: 'p3', price: 0.333 },
      ],
      0.7
    );
    expect(allocationSum(swap)).toBe(toStroops(0.7));
  });

  test('property: allocations always sum to totalPrice in stroops', () => {
    fc.assert(
      fc.property(
        fc.array(fc.integer({ min: 1, max: 10_000_000_000 }), { minLength: 1, maxLength: 8 }),
        fc.integer({ min: 1, max: 10_000_000_000 }),
        (prices, totalStroops) => {
          const assets = prices.map((p, i) => ({ assetId: `asset-${i}`, price: p / 1e8 }));
          const swap = swapWith(assets, totalStroops / 1e8);
          return allocationSum(swap) === BigInt(totalStroops);
        }
      ),
      { numRuns: 300 }
    );
  });
});
