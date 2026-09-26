# Commitment Proof of Non-Existence (#1071)

Proving an IP commitment exists is straightforward: the `IpRecord` is on-chain.
Proving that a piece of data had **not** been committed before a given date is
harder. The registry supports this through accumulated Merkle snapshots.

## Flow

1. **Snapshot** — anyone calls `take_commitment_snapshot()`. The contract builds a
   binary Merkle tree over every commitment hash registered so far (in IP ID
   order) and stores a `MerkleSnapshotRecord { snapshot_id, merkle_root,
   timestamp, commitment_count, max_ip_id }`.
2. **Generate** — `generate_non_existence_proof(data_hash, timestamp) -> Bytes`
   picks the earliest snapshot taken at or after `timestamp`, checks that
   `data_hash` was not committed by any IP covered by it, and returns an
   XDR-encoded `NonExistenceProof`.
3. **Verify** — `verify_non_existence(proof: Bytes) -> bool` decodes the proof,
   checks it against the stored snapshot (root, timestamp, coverage of the
   claimed time), recomputes the binding digest and re-checks absence.

The binding digest is:

```
proof_bytes = sha256(data_hash || snapshot_root || snapshot_id_be || claimed_before_be)
```

## Privacy guarantees

- **Data is never revealed.** Only the SHA-256 `data_hash` is handled. The
  underlying document, secret or blinding factor is never sent on-chain.
- **No leakage about other commitments.** A proof exposes only the snapshot root
  and commitment count, which are already public on-chain. It contains no
  sibling paths or other commitment hashes.
- **Non-replayable.** `proof_bytes` binds the hash to one snapshot and one
  claimed timestamp, so a proof cannot be re-used for a different snapshot or
  date.
- **Caveat.** `data_hash` is itself public in the proof. If the underlying data
  has low entropy (e.g. a short word), an observer could brute-force it. Callers
  should hash data together with a random salt, as with ordinary commitments
  (see [commitment-scheme.md](commitment-scheme.md)).

## Limitations

- A snapshot must exist at or after the claimed timestamp; otherwise no proof
  can be generated. Operators should take snapshots periodically.
- Absence is checked by scanning IP records up to `max_ip_id`, so cost grows
  linearly with the number of commitments covered by the snapshot.
