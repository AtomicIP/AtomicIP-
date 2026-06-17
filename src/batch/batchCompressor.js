/**
 * Swap Batch Compression (#525)
 * ─────────────────────────────
 * Off-chain utility to compress / decompress arrays of swap records using
 * Node's built-in zlib (deflate/inflate) so no extra dependencies are needed.
 *
 * Compression is applied to the JSON-serialised batch before transmission or
 * storage, reducing payload size for large batches.
 */

const zlib = require("zlib");

const MAX_BATCH_SIZE = 100;

// ── Helpers ───────────────────────────────────────────────────────────────────

function validateSwaps(swaps) {
  if (!Array.isArray(swaps) || swaps.length === 0)
    throw new TypeError("swaps must be a non-empty array.");
  if (swaps.length > MAX_BATCH_SIZE)
    throw new RangeError(`Batch size ${swaps.length} exceeds maximum of ${MAX_BATCH_SIZE}.`);
  for (let i = 0; i < swaps.length; i++) {
    if (!swaps[i] || typeof swaps[i] !== "object")
      throw new TypeError(`Swap at index ${i} must be an object.`);
  }
}

// ── Core ──────────────────────────────────────────────────────────────────────

/**
 * Compress an array of swap records into a Buffer.
 *
 * @param {object[]} swaps
 * @returns {Buffer} deflate-compressed bytes
 */
function compressBatchSwaps(swaps) {
  validateSwaps(swaps);
  const json = JSON.stringify(swaps);
  return zlib.deflateSync(Buffer.from(json, "utf8"));
}

/**
 * Decompress a Buffer produced by compressBatchSwaps back into swap records.
 *
 * @param {Buffer} compressed
 * @returns {object[]}
 */
function decompressBatchSwaps(compressed) {
  if (!Buffer.isBuffer(compressed) && !(compressed instanceof Uint8Array))
    throw new TypeError("compressed must be a Buffer.");
  const json = zlib.inflateSync(compressed).toString("utf8");
  const swaps = JSON.parse(json);
  if (!Array.isArray(swaps))
    throw new Error("Decompressed data is not an array.");
  return swaps;
}

module.exports = { compressBatchSwaps, decompressBatchSwaps, MAX_BATCH_SIZE };
