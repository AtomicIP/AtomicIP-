/**
 * Swap Matching Engine — Issue #475
 * ──────────────────────────────────
 * Matches buyers and sellers based on:
 *  - Asset type compatibility
 *  - Price range overlap
 *  - Category preferences
 *  - Condition requirements
 *  - Geographic proximity (optional)
 *
 * Scoring model (0–100):
 *  30pts — price overlap
 *  25pts — category match
 *  20pts — condition match
 *  15pts — asset type exact match
 *  10pts — location proximity
 */

const WEIGHTS = Object.freeze({
  PRICE:     30,
  CATEGORY:  25,
  CONDITION: 20,
  ASSET_TYPE: 15,
  LOCATION:  10,
});

const CONDITIONS_ORDER = ["poor", "fair", "good", "excellent"];
const MAX_RESULTS      = 50;
const MIN_MATCH_SCORE  = 40;

function validateListing(listing, role) {
  if (!listing || typeof listing !== "object")
    throw new TypeError(`${role} listing must be an object.`);
  if (!listing.id)
    throw new TypeError(`${role} listing: id is required.`);
  if (typeof listing.price !== "number" && typeof listing.maxPrice !== "number" && typeof listing.minPrice !== "number")
    throw new TypeError(`${role} listing ${listing.id}: price is required.`);
  if (!listing.assetType)
    throw new TypeError(`${role} listing ${listing.id}: assetType is required.`);
}

function scorePriceOverlap(buyer, seller) {
  const buyMax  = buyer.maxPrice  ?? buyer.price  ?? Infinity;
  const sellMin = seller.minPrice ?? seller.price ?? 0;

  if (buyMax >= sellMin) return WEIGHTS.PRICE;

  const gap = (sellMin - buyMax) / sellMin;
  if (gap <= 0.2) return Math.round(WEIGHTS.PRICE * (1 - gap / 0.2));
  return 0;
}

function scoreCategoryMatch(buyer, seller) {
  const buyerCats  = new Set((buyer.categories  ?? []).map((c) => c.toLowerCase()));
  const sellerCats = new Set((seller.categories ?? []).map((c) => c.toLowerCase()));

  const exactMatches = [...buyerCats].filter((c) => sellerCats.has(c)).length;
  if (exactMatches > 0)
    return Math.min(WEIGHTS.CATEGORY, Math.round(WEIGHTS.CATEGORY * (exactMatches / buyerCats.size || 1)));

  const buyerParents  = [...buyerCats].map((c)  => c.split(".")[0]);
  const sellerParents = [...sellerCats].map((c) => c.split(".")[0]);
  const parentMatch   = buyerParents.some((p) => sellerParents.includes(p));
  return parentMatch ? Math.round(WEIGHTS.CATEGORY * 0.4) : 0;
}

function scoreConditionMatch(buyer, seller) {
  const minIdx    = CONDITIONS_ORDER.indexOf((buyer.minCondition  ?? "fair").toLowerCase());
  const actualIdx = CONDITIONS_ORDER.indexOf((seller.condition ?? "good").toLowerCase());
  if (actualIdx < 0 || minIdx < 0) return Math.round(WEIGHTS.CONDITION * 0.5);
  if (actualIdx >= minIdx) return WEIGHTS.CONDITION;
  const deficit = minIdx - actualIdx;
  return Math.max(0, Math.round(WEIGHTS.CONDITION * (1 - deficit / CONDITIONS_ORDER.length)));
}

function scoreAssetTypeMatch(buyer, seller) {
  return buyer.assetType.toLowerCase() === seller.assetType.toLowerCase()
    ? WEIGHTS.ASSET_TYPE
    : 0;
}

function haversineKm(lat1, lon1, lat2, lon2) {
  const R    = 6371;
  const dLat = ((lat2 - lat1) * Math.PI) / 180;
  const dLon = ((lon2 - lon1) * Math.PI) / 180;
  const a =
    Math.sin(dLat / 2) ** 2 +
    Math.cos((lat1 * Math.PI) / 180) *
      Math.cos((lat2 * Math.PI) / 180) *
      Math.sin(dLon / 2) ** 2;
  return R * 2 * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a));
}

function scoreLocation(buyer, seller) {
  const bLoc = buyer.location;
  const sLoc = seller.location;
  if (!bLoc || !sLoc) return Math.round(WEIGHTS.LOCATION * 0.5);

  if (bLoc.country && sLoc.country && bLoc.country !== sLoc.country)
    return 0;

  if (bLoc.lat != null && bLoc.lon != null && sLoc.lat != null && sLoc.lon != null) {
    const km = haversineKm(bLoc.lat, bLoc.lon, sLoc.lat, sLoc.lon);
    const maxKm = buyer.maxDistanceKm ?? 500;
    if (km <= maxKm) return WEIGHTS.LOCATION;
    if (km <= maxKm * 2) return Math.round(WEIGHTS.LOCATION * 0.5);
    return 0;
  }

  return Math.round(WEIGHTS.LOCATION * 0.5);
}

function scoreMatch(buyer, seller) {
  const breakdown = {
    price:     scorePriceOverlap(buyer, seller),
    category:  scoreCategoryMatch(buyer, seller),
    condition: scoreConditionMatch(buyer, seller),
    assetType: scoreAssetTypeMatch(buyer, seller),
    location:  scoreLocation(buyer, seller),
  };
  const score = Object.values(breakdown).reduce((s, v) => s + v, 0);
  return { score, breakdown };
}

function findMatchesForBuyer(buyer, sellers, options = {}) {
  validateListing(buyer, "buyer");
  if (!Array.isArray(sellers)) throw new TypeError("sellers must be an array.");

  const minScore   = options.minScore   ?? MIN_MATCH_SCORE;
  const maxResults = options.maxResults ?? MAX_RESULTS;

  return sellers
    .filter((s) => {
      try { validateListing(s, "seller"); return true; }
      catch { return false; }
    })
    .map((seller) => {
      const { score, breakdown } = scoreMatch(buyer, seller);
      return { sellerId: seller.id, sellerListing: seller, score, breakdown };
    })
    .filter((r) => r.score >= minScore)
    .sort((a, b) => b.score - a.score)
    .slice(0, maxResults);
}

function batchMatch(buyers, sellers, options = {}) {
  if (!Array.isArray(buyers)  || buyers.length  === 0) throw new TypeError("buyers must be a non-empty array.");
  if (!Array.isArray(sellers) || sellers.length === 0) throw new TypeError("sellers must be a non-empty array.");

  const results = buyers.map((buyer) => {
    try {
      return { buyerId: buyer.id, matches: findMatchesForBuyer(buyer, sellers, options) };
    } catch {
      return { buyerId: buyer?.id ?? "unknown", matches: [], error: true };
    }
  });

  return { totalBuyers: buyers.length, totalSellers: sellers.length, results };
}

module.exports = {
  scoreMatch,
  findMatchesForBuyer,
  batchMatch,
  WEIGHTS,
  MIN_MATCH_SCORE,
  haversineKm,
};
