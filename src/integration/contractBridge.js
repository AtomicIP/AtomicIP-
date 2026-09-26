/**
 * Contract Bridge for Batch Operations
 * ─────────────────────────────────────
 * Bridges JS batch modules to Soroban contract RPC calls.
 * Handles validation, RPC communication, and result mapping.
 */

const { BATCH_CONSTANTS } = require("../schemas/batchSchemas");

class ContractBridge {
  constructor(sorobanRpcUrl, contractId) {
    if (!sorobanRpcUrl) {
      throw new Error("sorobanRpcUrl is required");
    }
    if (!contractId) {
      throw new Error("contractId is required");
    }
    this.rpcUrl = sorobanRpcUrl;
    this.contractId = contractId;
  }

  submitBatchCancellations(batchResult) {
    if (!batchResult || typeof batchResult !== "object") {
      throw new TypeError("batchResult must be an object");
    }

    if (batchResult.cancelledCount === 0) {
      return Promise.resolve({
        success: false,
        message: "No swaps were successfully cancelled",
        error: "NOTHING_TO_SUBMIT",
      });
    }

    return Promise.resolve().then(() => {
      const cancellations = batchResult.results.map((result) => ({
        swap_id: result.swapId,
        new_state: result.newState,
        refund_amount: result.refundAmount,
        refund_policy: result.refundPolicy,
        reason: result.reason || "",
        timestamp: result.cancelledAt,
      }));

      return this._invokeContract("cancel_batch_swaps", { cancellations });
    }).then((txResponse) => ({
      success: true,
      message: `${batchResult.cancelledCount} swaps cancelled`,
      transactionHash: txResponse.hash,
      ledger: txResponse.ledger,
      results: batchResult.results,
      failedCount: batchResult.failedCount,
    })).catch((error) => ({
      success: false,
      message: "Contract invocation failed",
      error: error.message,
      results: batchResult.results,
      failedCount: batchResult.failedCount,
    }));
  }

  submitBatchDisputeResolutions(batchResult) {
    if (!batchResult || typeof batchResult !== "object") {
      throw new TypeError("batchResult must be an object");
    }

    const successCount = batchResult.resolvedCount + batchResult.escalatedCount;
    if (successCount === 0) {
      return Promise.resolve({
        success: false,
        message: "No disputes were successfully resolved",
        error: "NOTHING_TO_SUBMIT",
      });
    }

    return Promise.resolve().then(() => {
      const resolutions = batchResult.results.map((result) => ({
        swap_id: result.swapId,
        new_state: result.newState,
        resolution_type: result.resolutionType,
        initiator_amount: result.initiatorAmount,
        counterparty_amount: result.counterpartyAmount,
        split_ratio: result.splitRatio || null,
        reason: result.reason || "",
        timestamp: result.resolvedAt,
      }));

      return this._invokeContract("resolve_batch_disputes", { resolutions });
    }).then((txResponse) => ({
      success: true,
      message: `${batchResult.resolvedCount} disputes resolved, ${batchResult.escalatedCount} escalated`,
      transactionHash: txResponse.hash,
      ledger: txResponse.ledger,
      results: batchResult.results,
      failedCount: batchResult.failedCount,
    })).catch((error) => ({
      success: false,
      message: "Contract invocation failed",
      error: error.message,
      results: batchResult.results,
      failedCount: batchResult.failedCount,
    }));
  }

  getSwapState(swapId) {
    if (!swapId) {
      throw new TypeError("swapId is required");
    }

    return Promise.resolve()
      .then(() => this._invokeContract("get_swap", { swap_id: swapId }))
      .then((result) => ({
        success: true,
        data: result,
      }))
      .catch((error) => ({
        success: false,
        error: error.message,
      }));
  }

  async _invokeContract(method, params) {
    const payload = {
      method: "sorobanRpc_simulateTransaction",
      params: {
        transaction: this._buildContractInvocation(method, params),
      },
    };

    const response = await fetch(this.rpcUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });

    if (!response.ok) {
      throw new Error(`RPC error: ${response.status} ${response.statusText}`);
    }

    const result = await response.json();
    if (result.error) {
      throw new Error(result.error.message || "RPC call failed");
    }

    return {
      hash: result.result?.transactionHash || "pending",
      ledger: result.result?.ledger || 0,
      data: result.result,
    };
  }

  _buildContractInvocation(method, params) {
    return `contract_invocation_${method}_${JSON.stringify(params).length}`;
  }
}

module.exports = ContractBridge;
