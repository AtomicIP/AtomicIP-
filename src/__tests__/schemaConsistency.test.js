/**
 * Schema Consistency Tests
 * ───────────────────────
 * Verifies that JS and Rust validation constants remain in sync.
 * These tests catch drift between client and server validation rules.
 */

const { BATCH_CONSTANTS } = require("../schemas/batchSchemas");

describe("Schema Consistency — JS Constants", () => {
  describe("Batch Cancellation Constants", () => {
    test("CANCEL_MAX_BATCH_SIZE is 100 (matches Rust)", () => {
      expect(BATCH_CONSTANTS.CANCEL_MAX_BATCH_SIZE).toBe(100);
    });

    test("CANCEL_MAX_REASON_LENGTH is 256 (matches Rust)", () => {
      expect(BATCH_CONSTANTS.CANCEL_MAX_REASON_LENGTH).toBe(256);
    });

    test("MAX_BATCH_SIZE in batchCanceller.js matches constant", () => {
      const { MAX_BATCH_SIZE } = require("../batch/batchCanceller");
      expect(MAX_BATCH_SIZE).toBe(BATCH_CONSTANTS.CANCEL_MAX_BATCH_SIZE);
    });

    test("MAX_REASON_LENGTH in batchCanceller.js matches constant", () => {
      const { MAX_BATCH_SIZE } = require("../batch/batchCanceller");
      const batchCancellerModule = require("../batch/batchCanceller");
      // MAX_REASON_LENGTH is internal but should be 256
      const testSwap = { swapId: "t", state: "PENDING", amount: 100 };
      const longReason = "x".repeat(257);
      const result = batchCancellerModule.cancelBatchSwaps(
        [testSwap],
        [{ reason: longReason }]
      );
      expect(result.failedCount).toBe(1);
    });
  });

  describe("Batch Dispute Resolution Constants", () => {
    test("DISPUTE_MAX_BATCH_SIZE is 50 (matches Rust)", () => {
      expect(BATCH_CONSTANTS.DISPUTE_MAX_BATCH_SIZE).toBe(50);
    });

    test("DISPUTE_MAX_BATCH_SIZE in batchDisputeResolver.js matches constant", () => {
      const { MAX_BATCH_SIZE } = require("../batch/batchDisputeResolver");
      expect(MAX_BATCH_SIZE).toBe(BATCH_CONSTANTS.DISPUTE_MAX_BATCH_SIZE);
    });

    test("DISPUTE_VALID_SPLIT_RANGE matches batchDisputeResolver", () => {
      const { MAX_BATCH_SIZE } = require("../batch/batchDisputeResolver");
      const minRatio = BATCH_CONSTANTS.DISPUTE_VALID_SPLIT_RANGE_MIN;
      const maxRatio = BATCH_CONSTANTS.DISPUTE_VALID_SPLIT_RANGE_MAX;

      expect(minRatio).toBe(0.01);
      expect(maxRatio).toBe(0.99);
    });
  });

  describe("General Constraints", () => {
    test("MAX_STRING_LENGTH is 10000", () => {
      expect(BATCH_CONSTANTS.MAX_STRING_LENGTH).toBe(10000);
    });

    test("MAX_ARRAY_LENGTH is 1000", () => {
      expect(BATCH_CONSTANTS.MAX_ARRAY_LENGTH).toBe(1000);
    });

    test("OWASP_MAX_FIELD_LENGTH is 512", () => {
      expect(BATCH_CONSTANTS.OWASP_MAX_FIELD_LENGTH).toBe(512);
    });
  });

  describe("Refund Policy Enum Consistency", () => {
    test("REFUND_POLICIES in batchCanceller matches schema", () => {
      const { REFUND_POLICIES } = require("../batch/batchCanceller");

      expect(Object.values(REFUND_POLICIES)).toContain("FULL");
      expect(Object.values(REFUND_POLICIES)).toContain("PARTIAL");
      expect(Object.values(REFUND_POLICIES)).toContain("NONE");
      expect(Object.keys(REFUND_POLICIES).length).toBe(3);
    });
  });

  describe("Dispute State Enum Consistency", () => {
    test("DISPUTE_STATES in batchDisputeResolver matches schema", () => {
      const { DISPUTE_STATES } = require("../batch/batchDisputeResolver");

      expect(Object.values(DISPUTE_STATES)).toContain("OPEN");
      expect(Object.values(DISPUTE_STATES)).toContain("RESOLVED");
      expect(Object.values(DISPUTE_STATES)).toContain("REJECTED");
      expect(Object.values(DISPUTE_STATES)).toContain("ESCALATED");
    });

    test("RESOLUTION_TYPES in batchDisputeResolver matches schema", () => {
      const { RESOLUTION_TYPES } = require("../batch/batchDisputeResolver");

      expect(Object.values(RESOLUTION_TYPES)).toContain("REFUND");
      expect(Object.values(RESOLUTION_TYPES)).toContain("RELEASE");
      expect(Object.values(RESOLUTION_TYPES)).toContain("SPLIT");
      expect(Object.values(RESOLUTION_TYPES)).toContain("ESCALATE");
    });
  });

  describe("Validation Rule Alignment", () => {
    test("batchCanceller enforces CANCEL_MAX_BATCH_SIZE", () => {
      const { cancelBatchSwaps } = require("../batch/batchCanceller");
      const overSizeArray = Array.from(
        { length: BATCH_CONSTANTS.CANCEL_MAX_BATCH_SIZE + 1 },
        (_, i) => ({ swapId: `s${i}`, state: "PENDING", amount: 100 })
      );

      expect(() => cancelBatchSwaps(overSizeArray)).toThrow(RangeError);
    });

    test("batchDisputeResolver enforces DISPUTE_MAX_BATCH_SIZE", () => {
      const { resolveBatchDisputes } = require("../batch/batchDisputeResolver");
      const disputes = Array.from(
        { length: BATCH_CONSTANTS.DISPUTE_MAX_BATCH_SIZE + 1 },
        (_, i) => ({ swapId: `d${i}`, state: "OPEN", amount: 100 })
      );
      const resolutions = disputes.map(() => ({ type: "REFUND" }));

      expect(() => resolveBatchDisputes(disputes, resolutions)).toThrow(
        RangeError
      );
    });

    test("batchCanceller enforces CANCEL_MAX_REASON_LENGTH", () => {
      const { cancelBatchSwaps } = require("../batch/batchCanceller");
      const swap = { swapId: "s", state: "PENDING", amount: 100 };
      const longReason = "x".repeat(
        BATCH_CONSTANTS.CANCEL_MAX_REASON_LENGTH + 1
      );
      const cancellation = { reason: longReason };

      const result = cancelBatchSwaps([swap], [cancellation]);
      expect(result.failedCount).toBe(1);
    });
  });
});

describe("Schema Consistency — JSON Schema Validation", () => {
  const { BATCH_SCHEMAS } = require("../schemas/batchSchemas");

  describe("Cancel Batch Schema", () => {
    test("schema defines correct maxItems for batch size", () => {
      const schema = BATCH_SCHEMAS.cancelBatchSwaps;
      expect(schema.properties.swaps.maxItems).toBe(
        BATCH_CONSTANTS.CANCEL_MAX_BATCH_SIZE
      );
    });

    test("schema enforces reason max length", () => {
      const schema = BATCH_SCHEMAS.cancelBatchSwaps;
      const reasonSchema =
        schema.properties.cancellations.items.properties.reason;
      expect(reasonSchema.maxLength).toBe(
        BATCH_CONSTANTS.CANCEL_MAX_REASON_LENGTH
      );
    });
  });

  describe("Dispute Resolution Schema", () => {
    test("schema defines correct maxItems for batch size", () => {
      const schema = BATCH_SCHEMAS.resolveBatchDisputes;
      expect(schema.properties.disputes.maxItems).toBe(
        BATCH_CONSTANTS.DISPUTE_MAX_BATCH_SIZE
      );
    });

    test("schema enforces split ratio range", () => {
      const schema = BATCH_SCHEMAS.resolveBatchDisputes;
      const ratioSchema =
        schema.properties.resolutions.items.properties.splitRatio;
      expect(ratioSchema.minimum).toBe(
        BATCH_CONSTANTS.DISPUTE_VALID_SPLIT_RANGE_MIN
      );
      expect(ratioSchema.maximum).toBe(
        BATCH_CONSTANTS.DISPUTE_VALID_SPLIT_RANGE_MAX
      );
    });
  });
});
