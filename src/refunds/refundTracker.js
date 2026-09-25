const REFUND_STATUSES = Object.freeze({
  PENDING: 'PENDING',
  PROCESSING: 'PROCESSING',
  COMPLETED: 'COMPLETED',
  FAILED: 'FAILED'
});

function createRefundRecord({ swapId, amount, policy, now = Date.now() }) {
  if (!swapId) {throw new TypeError('swapId is required.');}
  if (typeof amount !== 'number' || amount < 0)
  {throw new RangeError('refund amount must be non-negative.');}
  if (!policy) {throw new TypeError('refund policy is required.');}

  return {
    refundId: `refund-${swapId}-${now}`,
    swapId,
    amount,
    policy,
    status: REFUND_STATUSES.PENDING,
    createdAt: new Date(now).toISOString(),
    updatedAt: new Date(now).toISOString()
  };
}

function updateRefundStatus(refund, status, options = {}) {
  if (!refund || typeof refund !== 'object')
  {throw new TypeError('refund must be an object.');}
  if (!Object.values(REFUND_STATUSES).includes(status))
  {throw new TypeError(`Invalid refund status '${status}'.`);}
  if (status === REFUND_STATUSES.FAILED && !options.reason)
  {throw new TypeError('A failure reason is required.');}

  const now = options.now ?? Date.now();
  return {
    ...refund,
    status,
    updatedAt: new Date(now).toISOString(),
    ...(status === REFUND_STATUSES.COMPLETED ? { completedAt: new Date(now).toISOString() } : {}),
    ...(status === REFUND_STATUSES.FAILED ? { failureReason: options.reason } : {}),
    ...(options.transactionId ? { transactionId: options.transactionId } : {})
  };
}

function summarizeRefunds(refunds) {
  if (!Array.isArray(refunds)) {throw new TypeError('refunds must be an array.');}
  const byStatus = Object.fromEntries(Object.values(REFUND_STATUSES).map((status) => [status, 0]));
  for (const refund of refunds) {
    if (!Object.prototype.hasOwnProperty.call(byStatus, refund.status))
    {throw new TypeError(`Unknown refund status '${refund.status}'.`);}
    byStatus[refund.status] += 1;
  }
  return {
    count: refunds.length,
    totalAmount: refunds.reduce((sum, refund) => sum + refund.amount, 0),
    byStatus
  };
}

module.exports = {
  REFUND_STATUSES,
  createRefundRecord,
  updateRefundStatus,
  summarizeRefunds
};
