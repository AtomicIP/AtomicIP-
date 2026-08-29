/**
 * Shared Batch Operation Schemas
 * ──────────────────────────────
 * JSON Schema definitions for batch operations shared between JS and API server.
 * These constants are mirrored in api-server/src/batch.rs to ensure consistency.
 */

const BATCH_CONSTANTS = Object.freeze({
  // Batch Cancellation
  CANCEL_MAX_BATCH_SIZE: 100,
  CANCEL_MAX_REASON_LENGTH: 256,

  // Batch Dispute Resolution
  DISPUTE_MAX_BATCH_SIZE: 50,
  DISPUTE_VALID_SPLIT_RANGE_MIN: 0.01,
  DISPUTE_VALID_SPLIT_RANGE_MAX: 0.99,

  // General constraints
  MAX_STRING_LENGTH: 10000,
  MAX_ARRAY_LENGTH: 1000,
  OWASP_MAX_FIELD_LENGTH: 512,
});

const BATCH_SCHEMAS = {
  // Batch Cancellation schema
  cancelBatchSwaps: {
    $schema: "http://json-schema.org/draft-07/schema#",
    title: "Cancel Batch Swaps Request",
    type: "object",
    properties: {
      swaps: {
        type: "array",
        minItems: 1,
        maxItems: BATCH_CONSTANTS.CANCEL_MAX_BATCH_SIZE,
        items: {
          type: "object",
          required: ["swapId", "state", "amount"],
          properties: {
            swapId: {
              type: "string",
              minLength: 1,
              maxLength: BATCH_CONSTANTS.OWASP_MAX_FIELD_LENGTH,
            },
            state: {
              type: "string",
              enum: ["PENDING", "ACTIVE", "COMPLETED", "EXPIRED", "CANCELLED"],
            },
            amount: {
              type: "number",
              minimum: 0,
            },
          },
        },
      },
      cancellations: {
        type: ["array", "null"],
        minItems: 0,
        maxItems: BATCH_CONSTANTS.CANCEL_MAX_BATCH_SIZE,
        items: {
          type: "object",
          properties: {
            reason: {
              type: "string",
              maxLength: BATCH_CONSTANTS.CANCEL_MAX_REASON_LENGTH,
            },
            refundPolicy: {
              type: "string",
              enum: ["FULL", "PARTIAL", "NONE"],
            },
            feePaid: {
              type: "number",
              minimum: 0,
            },
          },
        },
      },
    },
    required: ["swaps"],
  },

  // Batch Dispute Resolution schema
  resolveBatchDisputes: {
    $schema: "http://json-schema.org/draft-07/schema#",
    title: "Resolve Batch Disputes Request",
    type: "object",
    properties: {
      disputes: {
        type: "array",
        minItems: 1,
        maxItems: BATCH_CONSTANTS.DISPUTE_MAX_BATCH_SIZE,
        items: {
          type: "object",
          required: ["swapId", "state", "amount"],
          properties: {
            swapId: {
              type: "string",
              minLength: 1,
              maxLength: BATCH_CONSTANTS.OWASP_MAX_FIELD_LENGTH,
            },
            state: {
              type: "string",
              enum: ["OPEN", "RESOLVED", "REJECTED", "ESCALATED"],
            },
            amount: {
              type: "number",
              minimum: 0,
            },
          },
        },
      },
      resolutions: {
        type: "array",
        minItems: 1,
        maxItems: BATCH_CONSTANTS.DISPUTE_MAX_BATCH_SIZE,
        items: {
          type: "object",
          required: ["type"],
          properties: {
            type: {
              type: "string",
              enum: ["REFUND", "RELEASE", "SPLIT", "ESCALATE"],
            },
            splitRatio: {
              type: "number",
              minimum: BATCH_CONSTANTS.DISPUTE_VALID_SPLIT_RANGE_MIN,
              maximum: BATCH_CONSTANTS.DISPUTE_VALID_SPLIT_RANGE_MAX,
            },
            reason: {
              type: "string",
              maxLength: BATCH_CONSTANTS.CANCEL_MAX_REASON_LENGTH,
            },
          },
        },
      },
    },
    required: ["disputes", "resolutions"],
  },
};

module.exports = {
  BATCH_CONSTANTS,
  BATCH_SCHEMAS,
};
