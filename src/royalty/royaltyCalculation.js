/**
 * Shared Royalty Calculation — Issue #879
 * ────────────────────────────────────────
 * Single source of truth for royalty arithmetic, shared by both:
 *   - src/royalty/swapRoyaltyTracker.js
 *   - src/batch/batchRoyaltyDistributor.js
 *
 * Canonical formula
 * -----------------
 * Given:
 *   salePrice  — gross sale price (positive integer, token units)
 *   rateBps    — royalty rate in basis points (0–3000, i.e. 0–30%)
 *   BPS_DENOM  — 10 000 (one basis point = 1/10 000)
 *
 * Total royalty:
 *   totalRoyalty = floor(salePrice × rateBps / BPS_DENOM)
 *
 * Per-beneficiary payout (pro-rata by shareBps, each beneficiary's shareBps
 * values summing to BPS_DENOM):
 *   amount_i = floor(totalRoyalty × shareBps_i / BPS_DENOM)
 *
 * Dust (rounding residual):
 *   dust = totalRoyalty − Σ amount_i   [always 0 or 1 due to floor]
 *   Assigned entirely to the first beneficiary so total distributed == totalRoyalty.
 *
 * Seller proceeds:
 *   sellerProceeds = salePrice − totalRoyalty
 */

const BPS_DENOM           = 10_000;
const MAX_ROYALTY_RATE_BPS = 3_000; // 30% ceiling

/**
 * Calculate royalty payouts for a single transaction.
 *
 * @param {number} salePrice
 *   Gross sale price in token units. Must be a positive number.
 * @param {number} rateBps
 *   Royalty rate in basis points (0–3000).
 * @param {Array<{ id: string, shareBps: number }>} beneficiaries
 *   Non-empty list of beneficiaries whose shareBps values sum to BPS_DENOM.
 *
 * @returns {{
 *   totalRoyalty: number,
 *   sellerProceeds: number,
 *   payouts: Array<{ beneficiaryId: string, shareBps: number, amount: number }>
 * }}
 *
 * @throws {RangeError}  if salePrice ≤ 0 or rateBps is out of range
 * @throws {TypeError}   if beneficiaries is not a valid non-empty array
 */
function computeRoyaltyPayouts(salePrice, rateBps, beneficiaries) {
  // ── Validation ────────────────────────────────────────────────────────────
  if (typeof salePrice !== "number" || salePrice <= 0)
    throw new RangeError("salePrice must be a positive number.");
  if (typeof rateBps !== "number" || rateBps < 0 || rateBps > MAX_ROYALTY_RATE_BPS)
    throw new RangeError(`rateBps must be between 0 and ${MAX_ROYALTY_RATE_BPS}.`);
  if (!Array.isArray(beneficiaries) || beneficiaries.length === 0)
    throw new TypeError("beneficiaries must be a non-empty array.");

  // ── Core arithmetic ───────────────────────────────────────────────────────
  const totalRoyalty = Math.floor((salePrice * rateBps) / BPS_DENOM);

  const payouts = beneficiaries.map((b) => ({
    beneficiaryId: b.id,
    shareBps:      b.shareBps,
    amount:        Math.floor((totalRoyalty * b.shareBps) / BPS_DENOM),
  }));

  // Assign rounding dust (≤ 1 token unit) to the first beneficiary so that
  // Σ payouts[i].amount === totalRoyalty invariant holds exactly.
  const distributed = payouts.reduce((s, p) => s + p.amount, 0);
  if (distributed < totalRoyalty) {
    payouts[0].amount += totalRoyalty - distributed;
  }

  return {
    totalRoyalty,
    sellerProceeds: salePrice - totalRoyalty,
    payouts,
  };
}

module.exports = { computeRoyaltyPayouts, BPS_DENOM, MAX_ROYALTY_RATE_BPS };
