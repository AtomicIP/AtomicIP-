const {
  scoreRecommendation,
  rankCandidates,
  recommendSwaps,
} = require('../recommendations/swapRecommendations');

describe('swapRecommendations', () => {
  describe('scoreRecommendation', () => {
    it('throws a TypeError for non-object candidates', () => {
      expect(() => scoreRecommendation(null)).toThrow(TypeError);
      expect(() => scoreRecommendation(undefined)).toThrow(TypeError);
      expect(() => scoreRecommendation('candidate')).toThrow(TypeError);
      expect(() => scoreRecommendation(42)).toThrow(TypeError);
      expect(() => scoreRecommendation([])).toThrow(TypeError);
    });

    it('returns a numeric score for a valid candidate', () => {
      const candidate = {
        id: 'c1',
        distance: 10,
        rating: 4.5,
        availability: 1,
      };
      const score = scoreRecommendation(candidate);
      expect(typeof score).toBe('number');
      expect(Number.isNaN(score)).toBe(false);
    });
  });

  describe('rankCandidates', () => {
    it('ranks candidates in descending order by recommendationScore', () => {
      const candidates = [
        { id: 'low', recommendationScore: 10 },
        { id: 'high', recommendationScore: 90 },
        { id: 'mid', recommendationScore: 50 },
      ];

      const ranked = rankCandidates(candidates);

      expect(ranked.map((c) => c.id)).toEqual(['high', 'mid', 'low']);
    });

    it('respects context.limit', () => {
      const candidates = [
        { id: 'a', recommendationScore: 10 },
        { id: 'b', recommendationScore: 90 },
        { id: 'c', recommendationScore: 50 },
        { id: 'd', recommendationScore: 70 },
      ];

      const ranked = rankCandidates(candidates, { limit: 2 });

      expect(ranked).toHaveLength(2);
      expect(ranked.map((c) => c.id)).toEqual(['b', 'd']);
    });

    it('handles ties deterministically with a stable order', () => {
      const candidates = [
        { id: 'first', recommendationScore: 50 },
        { id: 'second', recommendationScore: 50 },
        { id: 'third', recommendationScore: 50 },
      ];

      const ranked = rankCandidates(candidates);

      expect(ranked.map((c) => c.id)).toEqual(['first', 'second', 'third']);

      const rankedAgain = rankCandidates(candidates);
      expect(rankedAgain.map((c) => c.id)).toEqual(['first', 'second', 'third']);
    });

    it('returns an empty array for an empty candidate list', () => {
      expect(rankCandidates([])).toEqual([]);
    });
  });

  describe('recommendSwaps', () => {
    it('throws for non-array input', () => {
      expect(() => recommendSwaps(null)).toThrow();
      expect(() => recommendSwaps(undefined)).toThrow();
      expect(() => recommendSwaps('candidates')).toThrow();
      expect(() => recommendSwaps({})).toThrow();
    });

    it('returns an empty array for an empty candidate list', () => {
      expect(recommendSwaps([])).toEqual([]);
    });

    it('returns ranked recommendations respecting context.limit', () => {
      const candidates = [
        { id: 'low', recommendationScore: 10 },
        { id: 'high', recommendationScore: 90 },
        { id: 'mid', recommendationScore: 50 },
      ];

      const result = recommendSwaps(candidates, { limit: 2 });

      expect(result).toHaveLength(2);
      expect(result.map((c) => c.id)).toEqual(['high', 'mid']);
    });

    it('handles ties deterministically', () => {
      const candidates = [
        { id: 'first', recommendationScore: 50 },
        { id: 'second', recommendationScore: 50 },
      ];

      const result = recommendSwaps(candidates);

      expect(result.map((c) => c.id)).toEqual(['first', 'second']);
    });
  });
});