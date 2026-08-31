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
  const matchedAt  = options.now ?? Date.now();

  return sellers
    .filter((s) => {
      try { validateListing(s, "seller"); return true; }
      catch { return false; }
    })
    .map((seller) => {
      const { score, breakdown } = scoreMatch(buyer, seller);
      return { sellerId: seller.id, sellerListing: seller, score, breakdown, matchedAt };
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

// ── On-chain reconciliation (#877) ──────────────────────────────────────────────
//
// The matching engine above works entirely off a point-in-time snapshot of
// buyer/seller listings. If the swap a seller listing represents is
// cancelled (or otherwise moves out of a matchable state) on-chain after
// that snapshot was taken but before the match is submitted, the engine has
// no way to know — and submitting against it would fail on-chain, or worse,
// race a legitimate state change. `reconcileMatchBeforeSubmission` re-checks
// a match's on-chain swap state immediately before submission so a stale
// match can be dropped instead of submitted.
//
// See docs/atomic-swap.md, "#877: Off-Chain Match Reconciliation with
// On-Chain State" for the staleness window and its handling.

// States a swap must be in on-chain for a match against it to still be
// submittable. Mirrors CANCELLABLE_STATES in batchCanceller.js: a swap that
// has moved past PENDING/ACTIVE (e.g. CANCELLED, COMPLETED, DISPUTED) is no
// longer something a fresh match can be submitted against.
const MATCHABLE_STATES = new Set(["PENDING", "ACTIVE"]);

// How long a match may sit between being produced (`findMatchesForBuyer`/
// `batchMatch`) and being submitted before it must be treated as stale on
// timing grounds alone, independent of any on-chain check. 30s comfortably
// covers normal UI/queueing latency while bounding how long a caller can
// act on a snapshot that's no longer necessarily true.
const MATCH_STALENESS_WINDOW_MS = 30_000;

function matchSwapId(match) {
  return match?.sellerListing?.swapId ?? match?.sellerId ?? null;
}

/**
 * Pure timing check: has a match aged past the staleness window since it was
 * produced? Matches without a `matchedAt` timestamp can't be judged this way
 * and are treated as not stale (callers relying on this should ensure their
 * match objects carry `matchedAt`, e.g. from `findMatchesForBuyer`).
 *
 * @param {{ matchedAt?: number }} match
 * @param {number} [now]
 * @param {number} [windowMs]
 * @returns {boolean}
 */
function isMatchStale(match, now = Date.now(), windowMs = MATCH_STALENESS_WINDOW_MS) {
  if (typeof match?.matchedAt !== "number") return false;
  return now - match.matchedAt > windowMs;
}

/**
 * Re-validate a single match's swap state on-chain.
 *
 * @param {object} match - a result entry from `findMatchesForBuyer`/`batchMatch`
 * @param {(swapId: string) => (string|Promise<string>)} getSwapState
 *        Injected on-chain state lookup (sync or async) so this stays
 *        testable without a real RPC client, e.g. a Soroban `AtomicSwap`
 *        client's `get_swap_status` call.
 * @returns {Promise<{ swapId: string, matchable: boolean, onChainState: string|null, reason: string|null }>}
 */
async function reconcileMatchOnChain(match, getSwapState) {
  if (typeof getSwapState !== "function")
    throw new TypeError("getSwapState must be a function.");

  const swapId = matchSwapId(match);
  if (!swapId)
    throw new TypeError("match must reference a sellerId/sellerListing.swapId to reconcile.");

  const onChainState = await getSwapState(swapId);
  const matchable     = MATCHABLE_STATES.has(onChainState);

  return {
    swapId,
    matchable,
    onChainState: onChainState ?? null,
    reason: matchable
      ? null
      : `Swap ${swapId} is '${onChainState}' on-chain and can no longer be matched.`,
  };
}

/**
 * The full pre-submission reconciliation check: reject a match outright if
 * it has gone stale by time, otherwise re-validate it against current
 * on-chain state.
 *
 * @param {object} match
 * @param {(swapId: string) => (string|Promise<string>)} getSwapState
 * @param {{ now?: number, staleWindowMs?: number }} [options]
 * @returns {Promise<{ swapId: string|null, submittable: boolean, reason: string|null, onChainState: string|null }>}
 */
async function reconcileMatchBeforeSubmission(match, getSwapState, options = {}) {
  const now      = options.now ?? Date.now();
  const windowMs = options.staleWindowMs ?? MATCH_STALENESS_WINDOW_MS;

  if (isMatchStale(match, now, windowMs)) {
    return {
      swapId:       matchSwapId(match),
      submittable:  false,
      reason:       `Match is older than the ${windowMs}ms staleness window; re-match before submitting.`,
      onChainState: null,
    };
  }

  const { swapId, matchable, onChainState, reason } = await reconcileMatchOnChain(match, getSwapState);
  return { swapId, submittable: matchable, reason, onChainState };
}

module.exports = {
  scoreMatch,
  findMatchesForBuyer,
  batchMatch,
  reconcileMatchOnChain,
  reconcileMatchBeforeSubmission,
  isMatchStale,
  WEIGHTS,
  MIN_MATCH_SCORE,
  MATCHABLE_STATES,
  MATCH_STALENESS_WINDOW_MS,
  haversineKm,
};
