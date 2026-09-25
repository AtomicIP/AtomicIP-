/**
 * Swap recommendation engine — Issue: improve matching quality
 */

function scoreRecommendation(candidate, context = {}) {
  if (!candidate || typeof candidate !== "object") {
    throw new TypeError("candidate must be an object.");
  }

  let score = 0;
  if (candidate.priceMatch === true) score += 30;
  if (candidate.categoryMatch === true) score += 25;
  if (candidate.conditionMatch === true) score += 20;
  if (candidate.assetTypeMatch === true) score += 15;
  if (candidate.locationMatch === true) score += 10;

  const recencyBoost = context.recentlyViewed === true ? 5 : 0;
  const ratingBoost = typeof candidate.userRating === "number" ? Math.min(candidate.userRating * 5, 10) : 0;
  return Math.min(score + recencyBoost + ratingBoost, 100);
}

function recommendSwaps(candidates = [], context = {}) {
  if (!Array.isArray(candidates)) {
    throw new TypeError("candidates must be an array.");
  }

  return candidates
    .map((candidate, index) => ({
      ...candidate,
      recommendationScore: scoreRecommendation(candidate, context),
      rank: index + 1,
    }))
    .sort((a, b) => b.recommendationScore - a.recommendationScore)
    .slice(0, context.limit ?? 10);
}

module.exports = {
  scoreRecommendation,
  recommendSwaps,
};
