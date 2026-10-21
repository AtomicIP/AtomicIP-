'use strict';

const {
  executeBulkSwap,
  validateSwapOperation,
} = require('../batch/bulkSwapOperations');

describe('bulkSwapOperations', () => {
  describe('validateSwapOperation', () => {
    it('rejects a swap missing swapId', () => {
      const result = validateSwapOperation({ action: 'swap' });
      expect(result.valid).toBe(false);
      expect(result.error).toMatch(/swapId/i);
    });

    it('rejects a swap missing action', () => {
      const result = validateSwapOperation({ swapId: 'swap-1' });
      expect(result.valid).toBe(false);
      expect(result.error).toMatch(/action/i);
    });

    it('accepts a valid swap operation', () => {
      const result = validateSwapOperation({ swapId: 'swap-1', action: 'swap' });
      expect(result.valid).toBe(true);
    });
  });

  describe('executeBulkSwap', () => {
    it('reports all-success batch with successCount equal to batchSize and empty errors', async () => {
      const swaps = [
        { swapId: 'swap-1', action: 'swap' },
        { swapId: 'swap-2', action: 'swap' },
        { swapId: 'swap-3', action: 'swap' },
      ];
      const executor = jest.fn().mockResolvedValue({ ok: true });

      const result = await executeBulkSwap(swaps, executor);

      expect(executor).toHaveBeenCalledTimes(swaps.length);
      expect(result.successCount).toBe(swaps.length);
      expect(result.batchSize).toBe(swaps.length);
      expect(result.errors).toEqual([]);
      expect(result.results).toHaveLength(swaps.length);
      result.results.forEach((r, i) => {
        expect(r.swapId).toBe(swaps[i].swapId);
        expect(r.status).toBe('success');
      });
    });

    it('reports partial-failure batch with per-item errors and successful results', async () => {
      const swaps = [
        { swapId: 'swap-1', action: 'swap' },
        { swapId: 'swap-2', action: 'swap' },
        { swapId: 'swap-3', action: 'swap' },
        { swapId: 'swap-4', action: 'swap' },
      ];
      const failingIds = new Set(['swap-2', 'swap-4']);
      const executor = jest.fn().mockImplementation(async (swap) => {
        if (failingIds.has(swap.swapId)) {
          throw new Error(`execution failed for ${swap.swapId}`);
        }
        return { ok: true };
      });

      const result = await executeBulkSwap(swaps, executor);

      expect(result.batchSize).toBe(swaps.length);
      expect(result.successCount).toBe(2);
      expect(result.errors).toHaveLength(2);

      const errorBySwapId = result.errors.reduce((acc, e) => {
        acc[e.swapId] = e;
        return acc;
      }, {});

      expect(errorBySwapId['swap-2']).toBeDefined();
      expect(errorBySwapId['swap-2'].index).toBe(1);
      expect(errorBySwapId['swap-2'].status).toBe('failed');
      expect(errorBySwapId['swap-2'].error).toBe(
        'execution failed for swap-2'
      );

      expect(errorBySwapId['swap-4']).toBeDefined();
      expect(errorBySwapId['swap-4'].index).toBe(3);
      expect(errorBySwapId['swap-4'].status).toBe('failed');
      expect(errorBySwapId['swap-4'].error).toBe(
        'execution failed for swap-4'
      );

      const successfulIds = result.results.map((r) => r.swapId);
      expect(successfulIds).toEqual(['swap-1', 'swap-3']);
      result.results.forEach((r) => expect(r.status).toBe('success'));
    });

    it('reports all-failure batch with every item in errors', async () => {
      const swaps = [
        { swapId: 'swap-1', action: 'swap' },
        { swapId: 'swap-2', action: 'swap' },
        { swapId: 'swap-3', action: 'swap' },
      ];
      const executor = jest.fn().mockImplementation(async (swap) => {
        throw new Error(`boom ${swap.swapId}`);
      });

      const result = await executeBulkSwap(swaps, executor);

      expect(result.batchSize).toBe(swaps.length);
      expect(result.successCount).toBe(0);
      expect(result.errors).toHaveLength(swaps.length);
      expect(result.results).toEqual([]);

      swaps.forEach((swap, i) => {
        const err = result.errors.find((e) => e.swapId === swap.swapId);
        expect(err).toBeDefined();
        expect(err.index).toBe(i);
        expect(err.status).toBe('failed');
        expect(err.error).toBe(`boom ${swap.swapId}`);
      });
    });

    it('throws a validation error when swaps is not an array', async () => {
      const executor = jest.fn();
      await expect(executeBulkSwap('not-an-array', executor)).rejects.toThrow(
        /array/i
      );
      expect(executor).not.toHaveBeenCalled();
    });

    it('throws a validation error when executor is not a function', async () => {
      const swaps = [{ swapId: 'swap-1', action: 'swap' }];
      await expect(executeBulkSwap(swaps, 'not-a-function')).rejects.toThrow(
        /function/i
      );
    });
  });
});