/**
 * Integration Tests: Batch Modules ↔ Contract Bridge
 * ──────────────────────────────────────────────────
 * Verifies that batch module results properly wire to Soroban contracts.
 */

const ContractBridge = require("../integration/contractBridge");
const { cancelBatchSwaps } = require("../batch/batchCanceller");
const { resolveBatchDisputes } = require("../batch/batchDisputeResolver");

// Mock RPC response for testing
const mockRpcResponse = (txHash = "abc123") => {
  return {
    result: {
      transactionHash: txHash,
      ledger: 12345,
    },
  };
};

describe("ContractBridge", () => {
  let bridge;
  const mockRpcUrl = "http://localhost:8000/soroban/rpc";
  const mockContractId = "CAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB";

  beforeEach(() => {
    bridge = new ContractBridge(mockRpcUrl, mockContractId);

    // Mock fetch globally
    global.fetch = jest.fn((url, options) => {
      if (!options.body) return Promise.reject(new Error("No body"));

      const payload = JSON.parse(options.body);
      if (payload.error) {
        return Promise.resolve({
          ok: true,
          json: async () => ({
            error: { message: "RPC failed" },
          }),
        });
      }

      return Promise.resolve({
        ok: true,
        json: async () => mockRpcResponse(),
      });
    });
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe("constructor validation", () => {
    test("throws if sorobanRpcUrl is missing", () => {
      expect(() => new ContractBridge(null, "contract")).toThrow(
        "sorobanRpcUrl is required"
      );
    });

    test("throws if contractId is missing", () => {
      expect(
        () => new ContractBridge("http://localhost:8000", null)
      ).toThrow("contractId is required");
    });
  });

  describe("submitBatchCancellations — integration flow", () => {
    test("transforms batch result to contract format", async () => {
      const swaps = [
        { swapId: "swap1", state: "PENDING", amount: 1000 },
        { swapId: "swap2", state: "ACTIVE", amount: 500 },
      ];

      const batchResult = cancelBatchSwaps(swaps);
      const contractResult = await bridge.submitBatchCancellations(batchResult);

      expect(contractResult.success).toBe(true);
      expect(contractResult.cancelledCount).toBeUndefined();
      expect(contractResult.transactionHash).toBe("abc123");
      expect(contractResult.results).toHaveLength(2);
    });

    test("maps FULL refund policy to contract state", async () => {
      const swaps = [{ swapId: "s1", state: "PENDING", amount: 100 }];
      const cancellations = [{ refundPolicy: "FULL" }];

      const batchResult = cancelBatchSwaps(swaps, cancellations);
      const contractResult = await bridge.submitBatchCancellations(batchResult);

      expect(contractResult.success).toBe(true);
      expect(contractResult.results[0].refundAmount).toBe(100);
    });

    test("handles partial refunds correctly", async () => {
      const swaps = [{ swapId: "s2", state: "PENDING", amount: 1000 }];
      const cancellations = [{ refundPolicy: "PARTIAL", feePaid: 150 }];

      const batchResult = cancelBatchSwaps(swaps, cancellations);
      const contractResult = await bridge.submitBatchCancellations(batchResult);

      expect(contractResult.success).toBe(true);
      expect(contractResult.results[0].refundAmount).toBeCloseTo(850);
    });

    test("returns error if batch has no successful cancellations", async () => {
      const swaps = [{ swapId: "s3", state: "COMPLETED", amount: 100 }];

      const batchResult = cancelBatchSwaps(swaps);
      const contractResult = await bridge.submitBatchCancellations(batchResult);

      expect(contractResult.success).toBe(false);
      expect(contractResult.error).toBe("NOTHING_TO_SUBMIT");
    });
  });

  describe("submitBatchDisputeResolutions — integration flow", () => {
    test("transforms dispute resolution result to contract format", async () => {
      const disputes = [
        { swapId: "d1", state: "OPEN", amount: 500 },
        { swapId: "d2", state: "OPEN", amount: 800 },
      ];
      const resolutions = [
        { type: "REFUND" },
        { type: "RELEASE" },
      ];

      const batchResult = resolveBatchDisputes(disputes, resolutions);
      const contractResult = await bridge.submitBatchDisputeResolutions(
        batchResult
      );

      expect(contractResult.success).toBe(true);
      expect(contractResult.transactionHash).toBe("abc123");
      expect(contractResult.results).toHaveLength(2);
    });

    test("maps SPLIT resolution with ratio to contract state", async () => {
      const disputes = [{ swapId: "d3", state: "OPEN", amount: 1000 }];
      const resolutions = [{ type: "SPLIT", splitRatio: 0.6 }];

      const batchResult = resolveBatchDisputes(disputes, resolutions);
      const contractResult = await bridge.submitBatchDisputeResolutions(
        batchResult
      );

      expect(contractResult.success).toBe(true);
      expect(contractResult.results[0].splitRatio).toBe(0.6);
      expect(contractResult.results[0].initiatorAmount).toBeCloseTo(600);
      expect(contractResult.results[0].counterpartyAmount).toBeCloseTo(400);
    });

    test("handles ESCALATE resolution", async () => {
      const disputes = [{ swapId: "d4", state: "OPEN", amount: 2000 }];
      const resolutions = [{ type: "ESCALATE", reason: "Complex case" }];

      const batchResult = resolveBatchDisputes(disputes, resolutions);
      const contractResult = await bridge.submitBatchDisputeResolutions(
        batchResult
      );

      expect(contractResult.success).toBe(true);
      expect(contractResult.results[0].newState).toBe("ESCALATED");
    });
  });

  describe("getSwapState — query contract", () => {
    test("queries contract for swap state", async () => {
      const result = await bridge.getSwapState("swap123");

      expect(result.success).toBe(true);
      expect(result.data).toBeDefined();
    });

    test("throws if swapId is missing", () => {
      expect(() => bridge.getSwapState(null)).toThrow("swapId is required");
    });
  });

  describe("error handling", () => {
    test("handles RPC connection errors", async () => {
      global.fetch = jest.fn(() =>
        Promise.reject(new Error("Network error"))
      );

      const swaps = [{ swapId: "s", state: "PENDING", amount: 100 }];
      const batchResult = cancelBatchSwaps(swaps);

      const contractResult = await bridge.submitBatchCancellations(batchResult);
      expect(contractResult.success).toBe(false);
      expect(contractResult.error).toContain("Network error");
    });

    test("handles invalid batchResult", async () => {
      expect(
        () => bridge.submitBatchCancellations(null)
      ).toThrow("batchResult must be an object");
    });
  });
});
