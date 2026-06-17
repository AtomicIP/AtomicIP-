const {
  extendBatchExpiry,
  MAX_EXTENSION_MS,
  MIN_EXTENSION_MS,
  MAX_BATCH_SIZE,
} = require("../batch/batchExpiryExtender");

const NOW = new Date("2024-06-01T12:00:00.000Z").getTime();
const ONE_HOUR = 60 * 60 * 1000;
const ONE_DAY  = 24 * ONE_HOUR;

const pendingSwap = (id, offsetMs = ONE_DAY) => ({
  swapId:    id,
  state:     "PENDING",
  expiresAt: new Date(NOW + offsetMs).toISOString(),
});

describe("extendBatchExpiry — validation", () => {
  test("throws on empty array", () => {
    expect(() => extendBatchExpiry([], ONE_HOUR, { now: NOW })).toThrow(TypeError);
  });
  test("throws on batch > MAX_BATCH_SIZE", () => {
    const big = Array.from({ length: MAX_BATCH_SIZE + 1 }, (_, i) => pendingSwap(`s${i}`));
    expect(() => extendBatchExpiry(big, ONE_HOUR, { now: NOW })).toThrow(RangeError);
  });
  test("throws on extension below MIN_EXTENSION_MS", () => {
    expect(() =>
      extendBatchExpiry([pendingSwap("a")], MIN_EXTENSION_MS - 1, { now: NOW })
    ).toThrow(RangeError);
  });
  test("throws on extension above MAX_EXTENSION_MS", () => {
    expect(() =>
      extendBatchExpiry([pendingSwap("a")], MAX_EXTENSION_MS + 1, { now: NOW })
    ).toThrow(RangeError);
  });
  test("records error for non-extendable state", () => {
    const expired = { swapId: "ex1", state: "EXPIRED", expiresAt: new Date(NOW - ONE_DAY).toISOString() };
    const result = extendBatchExpiry([expired], ONE_HOUR, { now: NOW });
    expect(result.failedCount).toBe(1);
    expect(result.errors[0].swapId).toBe("ex1");
  });
});

describe("extendBatchExpiry — extension logic", () => {
  test("extends a single PENDING swap correctly", () => {
    const result = extendBatchExpiry([pendingSwap("p1")], ONE_HOUR, { now: NOW });
    expect(result.extendedCount).toBe(1);
    const newExpiry  = new Date(result.results[0].newExpiry).getTime();
    const prevExpiry = new Date(result.results[0].previousExpiry).getTime();
    expect(newExpiry - prevExpiry).toBe(ONE_HOUR);
  });

  test("applies single extension value to all swaps", () => {
    const swaps = [pendingSwap("b1"), pendingSwap("b2"), pendingSwap("b3")];
    const result = extendBatchExpiry(swaps, ONE_DAY, { now: NOW });
    expect(result.extendedCount).toBe(3);
    result.results.forEach((r) => expect(r.extensionMs).toBe(ONE_DAY));
  });

  test("applies per-swap extension array", () => {
    const swaps      = [pendingSwap("c1"), pendingSwap("c2")];
    const extensions = [ONE_HOUR, ONE_DAY];
    const result     = extendBatchExpiry(swaps, extensions, { now: NOW });
    expect(result.results[0].extensionMs).toBe(ONE_HOUR);
    expect(result.results[1].extensionMs).toBe(ONE_DAY);
  });

  test("totalExtensionMs sums all extensions", () => {
    const swaps = [pendingSwap("d1"), pendingSwap("d2")];
    const result = extendBatchExpiry(swaps, ONE_HOUR, { now: NOW });
    expect(result.totalExtensionMs).toBe(ONE_HOUR * 2);
  });

  test("ACTIVE swap is also extendable", () => {
    const active = { swapId: "act1", state: "ACTIVE", expiresAt: new Date(NOW + ONE_DAY).toISOString() };
    const result = extendBatchExpiry([active], ONE_HOUR, { now: NOW });
    expect(result.extendedCount).toBe(1);
  });
});

describe("extendBatchExpiry — mixed batch", () => {
  test("processes valid and invalid items, counts correctly", () => {
    const swaps = [
      pendingSwap("m1"),
      { swapId: "m2", state: "COMPLETED", expiresAt: new Date(NOW + ONE_DAY).toISOString() },
    ];
    const result = extendBatchExpiry(swaps, ONE_HOUR, { now: NOW });
    expect(result.extendedCount).toBe(1);
    expect(result.failedCount).toBe(1);
  });
});
