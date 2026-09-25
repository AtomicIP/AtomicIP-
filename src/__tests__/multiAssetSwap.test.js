const {
  MAX_ASSETS_PER_SWAP,
  validateAssetBundle,
  calculateBundleValue,
  createMultiAssetSwap,
  getAssetAllocation,
} = require('../swap/multiAssetSwap');

const assets = [
  { assetId: 'patent-1', price: 1000, title: 'Method A' },
  { assetId: 'trademark-1', price: 500, title: 'Brand B' },
];

describe('multi-asset swaps', () => {
  test('creates a bundled swap and calculates its value', () => {
    const swap = createMultiAssetSwap({
      swapId: 'bundle-1',
      seller: 'seller-1',
      buyer: 'buyer-1',
      assets,
      totalPrice: 1400,
    });

    expect(calculateBundleValue(assets)).toBe(1500);
    expect(swap.assetCount).toBe(2);
    expect(swap.bundleValue).toBe(1500);
    expect(swap.discount).toBe(100);
    expect(swap.status).toBe('PENDING');
  });

  test('allocates a bundle price proportionally for accounting', () => {
    const allocation = getAssetAllocation(createMultiAssetSwap({
      swapId: 'bundle-2',
      seller: 'seller-1',
      buyer: 'buyer-1',
      assets,
      totalPrice: 1200,
    }));

    expect(allocation.map((item) => item.allocatedPrice)).toEqual([800, 400]);
  });

  test('rejects duplicate assets and oversized bundles', () => {
    expect(() => validateAssetBundle([assets[0], assets[0]])).toThrow(/more than once/);
    expect(() => validateAssetBundle(
      Array.from({ length: MAX_ASSETS_PER_SWAP + 1 }, (_, index) => ({
        assetId: `asset-${index}`,
        price: 1,
      })),
    )).toThrow(/at most/);
  });
});
