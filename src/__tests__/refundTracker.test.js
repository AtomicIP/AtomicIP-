const {
  REFUND_STATUSES,
  createRefundRecord,
  updateRefundStatus,
  summarizeRefunds,
} = require('../refunds/refundTracker');
const { cancelBatchSwaps } = require('../batch/batchCanceller');

describe('refund tracking', () => {
  test('cancellation creates a pending refund record', () => {
    const result = cancelBatchSwaps(
      [{ swapId: 'swap-1', state: 'PENDING', amount: 100 }],
      [null],
      { now: 1_700_000_000_000 },
    );

    expect(result.refunds[0].status).toBe(REFUND_STATUSES.PENDING);
    expect(result.refundSummary.byStatus.PENDING).toBe(1);
  });

  test('tracks processing and completion metadata', () => {
    const refund = createRefundRecord({
      swapId: 'swap-2',
      amount: 250,
      policy: 'FULL',
      now: 1_700_000_000_000,
    });
    const processing = updateRefundStatus(refund, REFUND_STATUSES.PROCESSING, { now: 1_700_000_001_000 });
    const completed = updateRefundStatus(processing, REFUND_STATUSES.COMPLETED, {
      now: 1_700_000_002_000,
      transactionId: 'tx-1',
    });

    expect(completed.transactionId).toBe('tx-1');
    expect(completed.completedAt).toBeDefined();
  });

  test('requires a reason for failed refunds and summarizes amounts', () => {
    const refund = createRefundRecord({ swapId: 'swap-3', amount: 50, policy: 'PARTIAL' });
    expect(() => updateRefundStatus(refund, REFUND_STATUSES.FAILED)).toThrow(/reason/i);
    expect(summarizeRefunds([refund])).toEqual({
      count: 1,
      totalAmount: 50,
      byStatus: { PENDING: 1, PROCESSING: 0, COMPLETED: 0, FAILED: 0 },
    });
  });
});
