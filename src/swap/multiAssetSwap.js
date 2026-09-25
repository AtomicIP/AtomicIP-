const MAX_ASSETS_PER_SWAP = 50;

function validateAssetBundle(assets) {
  if (!Array.isArray(assets) || assets.length === 0)
  {throw new TypeError('assets must be a non-empty array.');}
  if (assets.length > MAX_ASSETS_PER_SWAP)
  {throw new RangeError(`A swap may contain at most ${MAX_ASSETS_PER_SWAP} assets.`);}

  const ids = new Set();
  for (const [index, asset] of assets.entries()) {
    if (!asset || typeof asset !== 'object' || !asset.assetId)
    {throw new TypeError(`Asset at index ${index}: assetId is required.`);}
    if (ids.has(asset.assetId))
    {throw new TypeError(`Asset '${asset.assetId}' appears more than once.`);}
    if (typeof asset.price !== 'number' || asset.price <= 0)
    {throw new RangeError(`Asset ${asset.assetId}: price must be positive.`);}
    ids.add(asset.assetId);
  }
  return true;
}

function calculateBundleValue(assets) {
  validateAssetBundle(assets);
  return assets.reduce((total, asset) => total + asset.price, 0);
}

function createMultiAssetSwap(request) {
  if (!request || typeof request !== 'object')
  {throw new TypeError('swap request must be an object.');}
  if (!request.swapId) {throw new TypeError('swapId is required.');}
  if (!request.seller) {throw new TypeError('seller is required.');}
  if (!request.buyer) {throw new TypeError('buyer is required.');}

  validateAssetBundle(request.assets);
  const calculatedValue = calculateBundleValue(request.assets);
  if (request.totalPrice !== null && request.totalPrice !== undefined &&
      (typeof request.totalPrice !== 'number' || request.totalPrice <= 0))
  {throw new RangeError('totalPrice must be positive.');}

  const totalPrice = request.totalPrice ?? calculatedValue;
  return {
    swapId: request.swapId,
    seller: request.seller,
    buyer: request.buyer,
    assets: request.assets.map((asset) => ({ ...asset })),
    assetCount: request.assets.length,
    bundleValue: calculatedValue,
    totalPrice,
    discount: calculatedValue - totalPrice,
    token: request.token ?? null,
    status: 'PENDING',
    createdAt: request.createdAt ?? new Date().toISOString()
  };
}

function getAssetAllocation(swap) {
  if (!swap || !Array.isArray(swap.assets) || swap.totalPrice <= 0)
  {throw new TypeError('a valid multi-asset swap is required.');}
  const bundleValue = calculateBundleValue(swap.assets);
  return swap.assets.map((asset) => ({
    assetId: asset.assetId,
    listPrice: asset.price,
    allocatedPrice: +(swap.totalPrice * asset.price / bundleValue).toFixed(8)
  }));
}

module.exports = {
  MAX_ASSETS_PER_SWAP,
  validateAssetBundle,
  calculateBundleValue,
  createMultiAssetSwap,
  getAssetAllocation
};
