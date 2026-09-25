/**
 * Deferred batch operation scheduling.
 *
 * Scheduling is deliberately explicit: callers provide an executor and invoke
 * runDue when their worker/queue is ready to process scheduled work.
 */

const SCHEDULED = 'SCHEDULED';
const RUNNING = 'RUNNING';
const COMPLETED = 'COMPLETED';
const FAILED = 'FAILED';
const CANCELLED = 'CANCELLED';
const MAX_BATCH_SIZE = 100;

function validateOperation(operation) {
  if (!operation || typeof operation !== 'object')
  {throw new TypeError('operation must be an object.');}
  if (!operation.batchId || typeof operation.batchId !== 'string')
  {throw new TypeError('operation.batchId must be a non-empty string.');}
  if (!Array.isArray(operation.operations) || operation.operations.length === 0)
  {throw new TypeError('operation.operations must be a non-empty array.');}
  if (operation.operations.length > MAX_BATCH_SIZE)
  {throw new RangeError(`Batch size ${operation.operations.length} exceeds maximum of ${MAX_BATCH_SIZE}.`);}
}

class BatchScheduler {
  constructor() {
    this.operations = new Map();
  }

  schedule(operation, executeAt, executor) {
    validateOperation(operation);
    if (!(executeAt instanceof Date) || Number.isNaN(executeAt.getTime()))
    {throw new TypeError('executeAt must be a valid Date.');}
    if (typeof executor !== 'function')
    {throw new TypeError('executor must be a function.');}
    if (this.operations.has(operation.batchId))
    {throw new Error(`Batch ${operation.batchId} is already scheduled.`);}

    const record = {
      ...operation,
      executeAt: executeAt.toISOString(),
      status: SCHEDULED,
      scheduledAt: new Date().toISOString(),
      executor
    };
    this.operations.set(operation.batchId, record);
    return this.getStatus(operation.batchId);
  }

  getStatus(batchId) {
    const record = this.operations.get(batchId);
    if (!record) {return null;}
    const status = { ...record };
    delete status.executor;
    return { ...status };
  }

  cancel(batchId) {
    const record = this.operations.get(batchId);
    if (!record) {throw new Error(`Batch ${batchId} was not found.`);}
    if (record.status !== SCHEDULED)
    {throw new Error(`Batch ${batchId} cannot be cancelled in state ${record.status}.`);}
    record.status = CANCELLED;
    record.cancelledAt = new Date().toISOString();
    return this.getStatus(batchId);
  }

  async runDue(now = new Date()) {
    if (!(now instanceof Date) || Number.isNaN(now.getTime()))
    {throw new TypeError('now must be a valid Date.');}

    const due = [...this.operations.values()]
      .filter((record) => record.status === SCHEDULED && new Date(record.executeAt) <= now)
      .sort((a, b) => a.executeAt.localeCompare(b.executeAt));

    const results = [];
    for (const record of due) {
      record.status = RUNNING;
      record.startedAt = now.toISOString();
      try {
        record.result = await record.executor(record.operations);
        record.status = COMPLETED;
      } catch (error) {
        record.status = FAILED;
        record.error = error instanceof Error ? error.message : String(error);
      }
      record.completedAt = now.toISOString();
      results.push(this.getStatus(record.batchId));
    }
    return results;
  }
}

module.exports = {
  BatchScheduler,
  SCHEDULED,
  RUNNING,
  COMPLETED,
  FAILED,
  CANCELLED,
  MAX_BATCH_SIZE
};
