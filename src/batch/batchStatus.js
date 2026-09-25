/**
 * In-memory batch lifecycle and progress tracking.
 *
 * The tracker is storage-agnostic so callers can persist snapshots or expose
 * getStatus/listStatuses through their monitoring endpoint.
 */

const STATES = Object.freeze([
  "QUEUED",
  "RUNNING",
  "COMPLETED",
  "FAILED",
  "CANCELLED",
]);

class BatchStatusTracker {
  constructor() {
    this.batches = new Map();
  }

  create(batchId, operationIds) {
    if (typeof batchId !== "string" || !batchId)
      throw new TypeError("batchId must be a non-empty string.");
    if (!Array.isArray(operationIds) || operationIds.length === 0)
      throw new TypeError("operationIds must be a non-empty array.");
    if (this.batches.has(batchId))
      throw new Error(`Batch ${batchId} already exists.`);

    const status = {
      batchId,
      state: "QUEUED",
      total: operationIds.length,
      completed: 0,
      failed: 0,
      cancelled: 0,
      operations: operationIds.map((id) => ({ id, state: "QUEUED" })),
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };
    this.batches.set(batchId, status);
    return this.getStatus(batchId);
  }

  update(batchId, operationId, state, details = {}) {
    if (!STATES.includes(state))
      throw new TypeError(`Invalid batch state '${state}'.`);
    const batch = this.batches.get(batchId);
    if (!batch) throw new Error(`Batch ${batchId} was not found.`);
    const operation = batch.operations.find(({ id }) => id === operationId);
    if (!operation) throw new Error(`Operation ${operationId} was not found in batch ${batchId}.`);

    operation.state = state;
    Object.assign(operation, details);
    batch.state = state === "RUNNING" ? "RUNNING" : batch.state;
    batch.completed = batch.operations.filter((item) => item.state === "COMPLETED").length;
    batch.failed = batch.operations.filter((item) => item.state === "FAILED").length;
    batch.cancelled = batch.operations.filter((item) => item.state === "CANCELLED").length;
    if (batch.completed + batch.failed + batch.cancelled === batch.total) {
      batch.state = batch.failed > 0 ? "FAILED" : batch.cancelled === batch.total ? "CANCELLED" : "COMPLETED";
    }
    batch.updatedAt = new Date().toISOString();
    return this.getStatus(batchId);
  }

  getStatus(batchId) {
    const status = this.batches.get(batchId);
    return status ? structuredClone(status) : null;
  }

  listStatuses() {
    return [...this.batches.keys()].map((batchId) => this.getStatus(batchId));
  }
}

module.exports = { BatchStatusTracker, STATES };
