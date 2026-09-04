#!/usr/bin/env node
/**
 * #882 — batchCompressor Benchmark
 * ─────────────────────────────────
 * Measures compression ratio and round-trip throughput for
 * batchCompressor.js across representative payload sizes.
 *
 * Run via jest (uses the project module system / babel-jest):
 *   npx jest --testPathPatterns="benchmark-batchCompressor"
 *
 * Or directly via the npm script:
 *   npm run benchmark:compressor
 *
 * Results are printed to stdout and the baseline table is written
 * (or updated) in docs/PERFORMANCE.md.
 */

"use strict";

const fs   = require("fs");
const path = require("path");
const {
  compressBatchSwaps,
  decompressBatchSwaps,
} = require("../src/batch/batchCompressor");

// ── Payload generators ────────────────────────────────────────────────────────

function makeSwap(i) {
  return {
    swapId:    `swap-${String(i).padStart(6, "0")}`,
    state:     i % 3 === 0 ? "COMPLETED" : i % 3 === 1 ? "PENDING" : "CANCELLED",
    amount:    1000 + (i * 7) % 900,
    salePrice: 5000 + (i * 13) % 45_000,
    seller:    "GCSELLER" + String(i).padStart(48, "X"),
    buyer:     "GCBUYER"  + String(i).padStart(49, "Y"),
    timestamp: 1_700_000_000 + i * 60,
    ipId:      i % 200,
  };
}

const SCENARIOS = [
  { label: "tiny  (  1 swap )", count:   1 },
  { label: "small ( 10 swaps)", count:  10 },
  { label: "mid   ( 50 swaps)", count:  50 },
  { label: "large (100 swaps)", count: 100 },
];

const RUNS = 200; // iterations for stable timing

// ── Core benchmark function ───────────────────────────────────────────────────

function bench(scenario) {
  const swaps    = Array.from({ length: scenario.count }, (_, i) => makeSwap(i));
  const rawJson  = JSON.stringify(swaps);
  const rawBytes = Buffer.byteLength(rawJson, "utf8");

  // Warmup
  for (let i = 0; i < 10; i++) decompressBatchSwaps(compressBatchSwaps(swaps));

  // Timed compress
  const t0 = process.hrtime.bigint();
  let compressed;
  for (let i = 0; i < RUNS; i++) compressed = compressBatchSwaps(swaps);
  const compressNs = Number(process.hrtime.bigint() - t0);

  // Timed decompress
  const t1 = process.hrtime.bigint();
  for (let i = 0; i < RUNS; i++) decompressBatchSwaps(compressed);
  const decompressNs = Number(process.hrtime.bigint() - t1);

  const roundTripped = decompressBatchSwaps(compressed);
  const ok           = JSON.stringify(roundTripped) === rawJson;

  return {
    label:           scenario.label,
    count:           scenario.count,
    rawBytes,
    compressedBytes: compressed.length,
    ratio:           rawBytes / compressed.length,
    compressMs:      compressNs / RUNS / 1e6,
    decompressMs:    decompressNs / RUNS / 1e6,
    roundTripOk:     ok,
  };
}

// ── Run benchmark and write PERFORMANCE.md ────────────────────────────────────

describe("#882 — batchCompressor benchmarks (compression ratio + round-trip timing)", () => {
  let results;

  beforeAll(() => {
    results = SCENARIOS.map(bench);
  });

  // ── Correctness assertions ────────────────────────────────────────────────

  for (const scenario of SCENARIOS) {
    test(`round-trip correctness: ${scenario.label}`, () => {
      const r = results.find((r) => r.label === scenario.label);
      expect(r.roundTripOk).toBe(true);
    });
  }

  // ── Compression ratio assertions ──────────────────────────────────────────

  test("mid  batch (50 swaps) compresses to < 50% of raw size", () => {
    const r = results.find((r) => r.count === 50);
    expect(r.compressedBytes).toBeLessThan(r.rawBytes * 0.50);
  });

  test("large batch (100 swaps) compresses to < 50% of raw size", () => {
    const r = results.find((r) => r.count === 100);
    expect(r.compressedBytes).toBeLessThan(r.rawBytes * 0.50);
  });

  test("compression ratio ≥ 1.5× for 100-swap batch", () => {
    const r = results.find((r) => r.count === 100);
    expect(r.ratio).toBeGreaterThanOrEqual(1.5);
  });

  // ── Performance ceiling assertions ───────────────────────────────────────

  test("compressing a 100-swap batch takes < 5 ms per operation", () => {
    const r = results.find((r) => r.count === 100);
    expect(r.compressMs).toBeLessThan(5);
  });

  test("decompressing a 100-swap batch takes < 5 ms per operation", () => {
    const r = results.find((r) => r.count === 100);
    expect(r.decompressMs).toBeLessThan(5);
  });

  // ── Print human-readable table + write PERFORMANCE.md ────────────────────

  afterAll(() => {
    if (!results) return;

    const COL = { label: 20, raw: 10, comp: 10, ratio: 7, cMs: 12, dMs: 14, ok: 5 };
    const header = [
      "Scenario".padEnd(COL.label),
      "Raw(B)".padStart(COL.raw),
      "Comp(B)".padStart(COL.comp),
      "Ratio".padStart(COL.ratio),
      "Compress(ms)".padStart(COL.cMs),
      "Decompress(ms)".padStart(COL.dMs),
      "RTok".padStart(COL.ok),
    ].join("  ");
    const sep = "-".repeat(header.length);

    console.log("\n  batchCompressor Benchmark Results");
    console.log("  " + sep);
    console.log("  " + header);
    console.log("  " + sep);
    for (const r of results) {
      console.log("  " + [
        r.label.padEnd(COL.label),
        String(r.rawBytes).padStart(COL.raw),
        String(r.compressedBytes).padStart(COL.comp),
        (r.ratio.toFixed(2) + "×").padStart(COL.ratio),
        (r.compressMs.toFixed(3) + "ms").padStart(COL.cMs),
        (r.decompressMs.toFixed(3) + "ms").padStart(COL.dMs),
        (r.roundTripOk ? "✓" : "✗").padStart(COL.ok),
      ].join("  "));
    }
    console.log("  " + sep);
    console.log(`  (averaged over ${RUNS} iterations · Node ${process.version})\n`);

    // Write to docs/PERFORMANCE.md
    const perfPath = path.join(__dirname, "..", "docs", "PERFORMANCE.md");
    const date     = new Date().toISOString().slice(0, 10);

    const tableLines = [
      `| Scenario | Raw (bytes) | Compressed (bytes) | Ratio | Compress (ms/op) | Decompress (ms/op) |`,
      `|----------|-------------|-------------------|-------|------------------|--------------------|`,
      ...results.map((r) =>
        `| ${r.label.trim()} | ${r.rawBytes} | ${r.compressedBytes} | ${r.ratio.toFixed(2)}× | ${r.compressMs.toFixed(3)} | ${r.decompressMs.toFixed(3)} |`
      ),
    ];

    const newSection = `
### batchCompressor baseline — ${date}

> Node ${process.version} · ${RUNS} iterations per scenario · zlib deflate (Node built-in)

${tableLines.join("\n")}
`;

    let existing = "";
    try { existing = fs.readFileSync(perfPath, "utf8"); } catch { /* new file */ }

    const SECTION_RE = /\n### batchCompressor baseline.*?(?=\n### |\n## |$)/s;
    const updated = SECTION_RE.test(existing)
      ? existing.replace(SECTION_RE, newSection)
      : existing + newSection;

    fs.writeFileSync(perfPath, updated, "utf8");
    console.log(`  Baseline written to docs/PERFORMANCE.md`);
  });
});
