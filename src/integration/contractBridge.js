/**
 * Contract Bridge for Batch Operations
 * ─────────────────────────────────────
 * Bridges JS batch modules to Soroban contract RPC calls.
 * Handles validation, RPC communication, and result mapping.
 */

const { BATCH_CONSTANTS } = require("../schemas/batchSchemas");
const {
  Account,
  Contract,
  Networks,
  StrKey,
  TransactionBuilder,
  BASE_FEE,
  nativeToScVal,
} = require("@stellar/stellar-sdk");

/**
 * Soroban integers cannot represent fractional JS numbers (e.g. a split
 * ratio of 0.6). Such values are sent as their decimal string so the
 * contract receives them losslessly; integers are encoded natively.
 */
function normalizeForScVal(value) {
  if (typeof value === "number" && !Number.isInteger(value)) {
    return String(value);
  }
  if (Array.isArray(value)) {
    return value.map(normalizeForScVal);
  }
  if (value && typeof value === "object" && Object.getPrototypeOf(value) === Object.prototype) {
    const out = {};
    for (const [k, v] of Object.entries(value)) {
      out[k] = normalizeForScVal(v);
    }
    return out;
  }
  return value;
}

// Placeholder source used only for simulation when no account is supplied.
// Simulation does not require a funded or signing account.
const SIMULATION_SOURCE = StrKey.encodeEd25519PublicKey(Buffer.alloc(32));

class ContractBridge {
  /**
   * @param {string} sorobanRpcUrl
   * @param {string} contractId      - Soroban contract address (C...)
   * @param {{ sourceAccount?: string, networkPassphrase?: string }} [options]
   */
  constructor(sorobanRpcUrl, contractId, options = {}) {
    if (!sorobanRpcUrl) {
      throw new Error("sorobanRpcUrl is required");
    }
    if (!contractId) {
      throw new Error("contractId is required");
    }
    this.rpcUrl = sorobanRpcUrl;
    this.contractId = contractId;
    this.sourceAccount = options.sourceAccount ?? SIMULATION_SOURCE;
    this.networkPassphrase = options.networkPassphrase ?? Networks.TESTNET;
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

  /**
   * Build a base64-encoded, unsigned Soroban transaction envelope that
   * invokes `method` on the bridge's contract with `params` as a map argument.
   */
  _buildContractInvocation(method, params) {
    const contract = new Contract(this.contractId);
    const args = params === undefined ? [] : [nativeToScVal(normalizeForScVal(params))];
    const tx = new TransactionBuilder(new Account(this.sourceAccount, "0"), {
      fee: BASE_FEE,
      networkPassphrase: this.networkPassphrase,
    })
      .addOperation(contract.call(method, ...args))
      .setTimeout(30)
      .build();
    return tx.toEnvelope().toXDR("base64");
  }
}

module.exports = ContractBridge;
