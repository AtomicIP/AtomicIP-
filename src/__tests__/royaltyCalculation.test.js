'use strict';

const {
  computeRoyaltyPayouts,
  BPS_DENOM,
  MAX_ROYALTY_RATE_BPS,
} = require('../royalty/royaltyCalculation');

describe('royaltyCalculation constants', () => {
  test('BPS_DENOM is 10000', () => {
    expect(BPS_DENOM).toBe(10000);
  });

  test('MAX_ROYALTY_RATE_BPS is 3000', () => {
    expect(MAX_ROYALTY_RATE_BPS).toBe(3000);
  });
});

describe('computeRoyaltyPayouts', () => {
  describe('fixed inputs', () => {
    test('computes payouts for a single beneficiary', () => {
      const result = computeRoyaltyPayouts({
        salePrice: 1000,
        rateBps: 500,
        beneficiaries: [{ address: '0xAAA', shareBps: 10000 }],
      });

      expect(result.totalRoyalty).toBe(50);
      expect(result.payouts).toEqual([{ address: '0xAAA', amount: 50 }]);
    });

    test('computes payouts for multiple beneficiaries', () => {
      const result = computeRoyaltyPayouts({
        salePrice: 10000,
        rateBps: 1000,
        beneficiaries: [
          { address: '0xAAA', shareBps: 5000 },
          { address: '0xBBB', shareBps: 3000 },
          { address: '0xCCC', shareBps: 2000 },
        ],
      });

      expect(result.totalRoyalty).toBe(1000);
      expect(result.payouts).toEqual([
        { address: '0xAAA', amount: 500 },
        { address: '0xBBB', amount: 300 },
        { address: '0xCCC', amount: 200 },
      ]);
    });

    test('sum of payouts equals totalRoyalty', () => {
      const result = computeRoyaltyPayouts({
        salePrice: 12345,
        rateBps: 750,
        beneficiaries: [
          { address: '0xAAA', shareBps: 4000 },
          { address: '0xBBB', shareBps: 6000 },
        ],
      });

      const sum = result.payouts.reduce((acc, p) => acc + p.amount, 0);
      expect(sum).toBe(result.totalRoyalty);
    });
  });

  describe('edge cases', () => {
    test('salePrice = 0 yields zero royalty and zero payouts', () => {
      const result = computeRoyaltyPayouts({
        salePrice: 0,
        rateBps: 500,
        beneficiaries: [
          { address: '0xAAA', shareBps: 5000 },
          { address: '0xBBB', shareBps: 5000 },
        ],
      });

      expect(result.totalRoyalty).toBe(0);
      expect(result.payouts).toEqual([
        { address: '0xAAA', amount: 0 },
        { address: '0xBBB', amount: 0 },
      ]);
    });

    test('rateBps = 0 yields zero royalty and zero payouts', () => {
      const result = computeRoyaltyPayouts({
        salePrice: 100000,
        rateBps: 0,
        beneficiaries: [
          { address: '0xAAA', shareBps: 5000 },
          { address: '0xBBB', shareBps: 5000 },
        ],
      });

      expect(result.totalRoyalty).toBe(0);
      expect(result.payouts).toEqual([
        { address: '0xAAA', amount: 0 },
        { address: '0xBBB', amount: 0 },
      ]);
    });

    test('rateBps = MAX_ROYALTY_RATE_BPS (3000) is accepted', () => {
      const result = computeRoyaltyPayouts({
        salePrice: 1000,
        rateBps: MAX_ROYALTY_RATE_BPS,
        beneficiaries: [{ address: '0xAAA', shareBps: 10000 }],
      });

      expect(result.totalRoyalty).toBe(300);
      expect(result.payouts).toEqual([{ address: '0xAAA', amount: 300 }]);
    });

    test('single beneficiary with full share receives entire royalty', () => {
      const result = computeRoyaltyPayouts({
        salePrice: 777,
        rateBps: 1000,
        beneficiaries: [{ address: '0xAAA', shareBps: 10000 }],
      });

      expect(result.totalRoyalty).toBe(77);
      expect(result.payouts).toEqual([{ address: '0xAAA', amount: 77 }]);
    });
  });

  describe('rounding and dust', () => {
    test('floors total royalty and assigns residual to first beneficiary', () => {
      // 999 * 333 / 10000 = 33.2667 -> floor 33
      const result = computeRoyaltyPayouts({
        salePrice: 999,
        rateBps: 333,
        beneficiaries: [
          { address: '0xAAA', shareBps: 5000 },
          { address: '0xBBB', shareBps: 5000 },
        ],
      });

      expect(result.totalRoyalty).toBe(33);
      const sum = result.payouts.reduce((acc, p) => acc + p.amount, 0);
      expect(sum).toBe(result.totalRoyalty);
      expect(result.payouts[0].address).toBe('0xAAA');
      expect(result.payouts[0].amount).toBe(17);
      expect(result.payouts[1].amount).toBe(16);
    });

    test('dust residual goes to first beneficiary with uneven shares', () => {
      // 1001 * 1000 / 10000 = 100.1 -> floor 100
      const result = computeRoyaltyPayouts({
        salePrice: 1001,
        rateBps: 1000,
        beneficiaries: [
          { address: '0xAAA', shareBps: 3333 },
          { address: '0xBBB', shareBps: 3333 },
          { address: '0xCCC', shareBps: 3334 },
        ],
      });

      expect(result.totalRoyalty).toBe(100);
      const sum = result.payouts.reduce((acc, p) => acc + p.amount, 0);
      expect(sum).toBe(result.totalRoyalty);
      expect(result.payouts[0].address).toBe('0xAAA');
      // floor(100 * 3333 / 10000) = 33, floor(100 * 3333 / 10000) = 33,
      // floor(100 * 3334 / 10000) = 33 -> residual 1 to first
      expect(result.payouts[0].amount).toBe(34);
      expect(result.payouts[1].amount).toBe(33);
      expect(result.payouts[2].amount).toBe(33);
    });

    test('invariant holds across many rounding cases', () => {
      const cases = [
        { salePrice: 1, rateBps: 1 },
        { salePrice: 3, rateBps: 333 },
        { salePrice: 7, rateBps: 1429 },
        { salePrice: 12345, rateBps: 2999 },
        { salePrice: 99999, rateBps: 1 },
      ];

      for (const { salePrice, rateBps } of cases) {
        const result = computeRoyaltyPayouts({
          salePrice,
          rateBps,
          beneficiaries: [
            { address: '0xAAA', shareBps: 3333 },
            { address: '0xBBB', shareBps: 3333 },
            { address: '0xCCC', shareBps: 3334 },
          ],
        });

        const sum = result.payouts.reduce((acc, p) => acc + p.amount, 0);
        expect(sum).toBe(result.totalRoyalty);
        expect(result.totalRoyalty).toBe(
          Math.floor((salePrice * rateBps) / BPS_DENOM)
        );
      }
    });
  });

  describe('validation errors', () => {
    test('throws on negative salePrice', () => {
      expect(() =>
        computeRoyaltyPayouts({
          salePrice: -1,
          rateBps: 500,
          beneficiaries: [{ address: '0xAAA', shareBps: 10000 }],
        })
      ).toThrow();
    });

    test('throws on rateBps above MAX_ROYALTY_RATE_BPS', () => {
      expect(() =>
        computeRoyaltyPayouts({
          salePrice: 1000,
          rateBps: MAX_ROYALTY_RATE_BPS + 1,
          beneficiaries: [{ address: '0xAAA', shareBps: 10000 }],
        })
      ).toThrow();
    });

    test('throws on negative rateBps', () => {
      expect(() =>
        computeRoyaltyPayouts({
          salePrice: 1000,
          rateBps: -1,
          beneficiaries: [{ address: '0xAAA', shareBps: 10000 }],
        })
      ).toThrow();
    });

    test('throws on empty beneficiaries', () => {
      expect(() =>
        computeRoyaltyPayouts({
          salePrice: 1000,
          rateBps: 500,
          beneficiaries: [],
        })
      ).toThrow();
    });

    test('throws on missing beneficiaries', () => {
      expect(() =>
        computeRoyaltyPayouts({
          salePrice: 1000,
          rateBps: 500,
        })
      ).toThrow();
    });
  });
});