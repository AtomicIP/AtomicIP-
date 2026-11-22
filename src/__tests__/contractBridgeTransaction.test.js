/**
 * Regression tests for issue #1322: ContractBridge sent a debug string
 * instead of a real Soroban transaction envelope to the RPC endpoint.
 */
const ContractBridge = require("../integration/contractBridge");
const {
  Address,
  Networks,
  Operation,
  Transaction,
  TransactionBuilder,
  scValToNative,
  StrKey,
} = require("@stellar/stellar-sdk");

const CONTRACT_ID = StrKey.encodeContract(Buffer.alloc(32, 7));
const RPC_URL = "https://rpc.example.test";

function decodeInvocation(envelope, passphrase = Networks.TESTNET) {
  const tx = TransactionBuilder.fromXDR(envelope, passphrase);
  expect(tx).toBeInstanceOf(Transaction);
  expect(tx.operations).toHaveLength(1);
  const op = tx.operations[0];
  expect(op.type).toBe("invokeHostFunction");
  const invoke = op.func.invokeContract();
  return {
    contract: Address.fromScAddress(invoke.contractAddress()).toString(),
    fn: invoke.functionName().toString(),
    args: invoke.args().map((a) => scValToNative(a)),
  };
}

describe("ContractBridge transaction construction (issue #1322)", () => {
  test("builds a decodable invokeHostFunction envelope for the contract", () => {
    const bridge = new ContractBridge(RPC_URL, CONTRACT_ID);
    const envelope = bridge._buildContractInvocation("get_swap", { swap_id: "s1" });
    const decoded = decodeInvocation(envelope);
    expect(decoded.contract).toBe(CONTRACT_ID);
    expect(decoded.fn).toBe("get_swap");
  });

  test("passes params as a single map argument", () => {
    const bridge = new ContractBridge(RPC_URL, CONTRACT_ID);
    const envelope = bridge._buildContractInvocation("get_swap", { swap_id: "s1" });
    const { args } = decodeInvocation(envelope);
    expect(args).toHaveLength(1);
    expect(args[0]).toEqual(expect.objectContaining({ swap_id: "s1" }));
  });

  test("never produces the old debug placeholder string", () => {
    const bridge = new ContractBridge(RPC_URL, CONTRACT_ID);
    const envelope = bridge._buildContractInvocation("get_swap", { swap_id: "s1" });
    expect(envelope).not.toMatch(/^contract_invocation_/);
    expect(() => TransactionBuilder.fromXDR(envelope, Networks.TESTNET)).not.toThrow();
  });

  test("uses the configured source account and network passphrase", () => {
    const source = StrKey.encodeEd25519PublicKey(Buffer.alloc(32, 1));
    const bridge = new ContractBridge(RPC_URL, CONTRACT_ID, {
      sourceAccount: source,
      networkPassphrase: Networks.PUBLIC,
    });
    const envelope = bridge._buildContractInvocation("get_swap", { swap_id: "s1" });
    const tx = TransactionBuilder.fromXDR(envelope, Networks.PUBLIC);
    expect(tx.source).toBe(source);
  });

  test("the RPC payload sent over fetch carries the real envelope", async () => {
    const bridge = new ContractBridge(RPC_URL, CONTRACT_ID);
    const fetchMock = jest.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ result: { transactionHash: "h", ledger: 1 } }),
    });
    const original = global.fetch;
    global.fetch = fetchMock;
    try {
      await bridge.getSwapState("s1");
    } finally {
      global.fetch = original;
    }
    const body = JSON.parse(fetchMock.mock.calls[0][1].body);
    expect(body.method).toBe("sorobanRpc_simulateTransaction");
    const decoded = decodeInvocation(body.params.transaction);
    expect(decoded.fn).toBe("get_swap");
    expect(decoded.contract).toBe(CONTRACT_ID);
  });

  test("rejects non-representable params instead of sending garbage", () => {
    const bridge = new ContractBridge(RPC_URL, CONTRACT_ID);
    expect(() => bridge._buildContractInvocation("get_swap", { x: Symbol("bad") })).toThrow();
  });
});

describe("ContractBridge fractional params (issue #1322)", () => {
  test("encodes a fractional splitRatio as its decimal string", () => {
    const bridge = new ContractBridge(RPC_URL, CONTRACT_ID);
    const envelope = bridge._buildContractInvocation("resolve", {
      swap_id: "d3",
      split_ratio: 0.6,
    });
    const { args } = decodeInvocation(envelope);
    expect(args[0]).toEqual(expect.objectContaining({ swap_id: "d3", split_ratio: "0.6" }));
  });

  test("keeps integer amounts as numbers", () => {
    const bridge = new ContractBridge(RPC_URL, CONTRACT_ID);
    const envelope = bridge._buildContractInvocation("resolve", { initiator_amount: 600 });
    const { args } = decodeInvocation(envelope);
    expect(args[0].initiator_amount).toBe(600n);
  });
});
