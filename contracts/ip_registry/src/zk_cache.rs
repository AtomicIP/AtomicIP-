//! Verification-result cache for hiding proofs (#1100).
//!
//! Checking a Schnorr proof costs several Ristretto scalar multiplications,
//! which is the most expensive part of `batch_verify_commitments`. Verdicts
//! are deterministic for a given `(commitment, proof)` pair, so they can be
//! cached.
//!
//! Design:
//!
//! * **Keys** are `sha256(epoch || commitment || R || s_secret || s_blinding)`.
//!   The epoch is stored per commitment, so a commitment change only needs to
//!   bump one counter to make every cached verdict for it unreachable. This
//!   avoids keeping an index of entries per commitment.
//! * **Invalidation** ([`invalidate`]) runs whenever an IP's commitment hash
//!   changes (`rotate_commitment_key` and the commitment-amendment path).
//! * **Size management**: at most [`ZK_CACHE_CAPACITY`] entries. Once full,
//!   the oldest entry is evicted (FIFO) so memory and storage stay bounded.
//! * **Tracking**: hits, misses, evictions and invalidations are kept in
//!   [`ZkCacheStats`], readable with [`stats`] and reported per batch as the
//!   `zk_cache` event.

use soroban_sdk::{contracttype, Bytes, BytesN, Env, Symbol};

use crate::{zk_commitment, DataKey, HidingCommitmentProof, LEDGER_BUMP};

/// Maximum number of cached verdicts held in storage at once.
pub const ZK_CACHE_CAPACITY: u32 = 256;

/// Running counters for the verification cache.
#[contracttype]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ZkCacheStats {
    /// Lookups answered from the cache.
    pub hits: u64,
    /// Lookups that ran a full verification.
    pub misses: u64,
    /// Entries currently occupying a slot, at most [`ZK_CACHE_CAPACITY`].
    pub entries: u32,
    /// Entries dropped to make room for new ones.
    pub evictions: u64,
    /// Commitment changes that invalidated cached verdicts.
    pub invalidations: u64,
}

impl ZkCacheStats {
    /// Hit rate in basis points (0..=10_000). Returns 0 before any lookup.
    pub fn hit_rate_bps(&self) -> u32 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0;
        }
        ((self.hits * 10_000) / total) as u32
    }
}

/// Read the current statistics. Intended for tests and internal reporting.
pub fn stats(env: &Env) -> ZkCacheStats {
    env.storage()
        .instance()
        .get(&DataKey::ZkCacheStats)
        .unwrap_or_default()
}

fn save_stats(env: &Env, stats: &ZkCacheStats) {
    env.storage().instance().set(&DataKey::ZkCacheStats, stats);
}

fn epoch_of(env: &Env, commitment: &BytesN<32>) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::ZkCacheEpoch(commitment.clone()))
        .unwrap_or(0)
}

/// Cache key binding the verdict to the commitment's current epoch.
fn entry_key(env: &Env, commitment: &BytesN<32>, proof: &HidingCommitmentProof) -> BytesN<32> {
    let epoch = epoch_of(env, commitment);
    let mut buf = Bytes::new(env);
    buf.append(&Bytes::from_array(env, &epoch.to_be_bytes()));
    buf.append(&commitment.clone().into());
    buf.append(&proof.r.clone().into());
    buf.append(&proof.s_secret.clone().into());
    buf.append(&proof.s_blinding.clone().into());
    env.crypto().sha256(&buf).into()
}

/// Verify a hiding proof, using the cache when a verdict is already stored.
pub fn verify_cached(env: &Env, commitment: &BytesN<32>, proof: &HidingCommitmentProof) -> bool {
    let key = entry_key(env, commitment, proof);
    let mut stats = stats(env);

    if let Some(cached) = env
        .storage()
        .persistent()
        .get::<_, bool>(&DataKey::ZkCacheEntry(key.clone()))
    {
        stats.hits += 1;
        save_stats(env, &stats);
        return cached;
    }

    let verdict = zk_commitment::verify_hiding_proof(env, commitment, proof);
    stats.misses += 1;
    insert(env, &mut stats, key, verdict);
    save_stats(env, &stats);
    verdict
}

fn insert(env: &Env, stats: &mut ZkCacheStats, key: BytesN<32>, verdict: bool) {
    let slot = if stats.entries < ZK_CACHE_CAPACITY {
        // Fill slots in order until the cache is full.
        let slot = stats.entries;
        stats.entries += 1;
        slot
    } else {
        // Full: overwrite the oldest slot, which is the cursor position.
        let cursor: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ZkCacheCursor)
            .unwrap_or(0);
        let slot_key = DataKey::ZkCacheSlot(cursor);
        if let Some(old_key) = env.storage().persistent().get::<_, BytesN<32>>(&slot_key) {
            env.storage()
                .persistent()
                .remove(&DataKey::ZkCacheEntry(old_key));
        }
        stats.evictions += 1;
        env.storage()
            .instance()
            .set(&DataKey::ZkCacheCursor, &((cursor + 1) % ZK_CACHE_CAPACITY));
        cursor
    };

    let slot_key = DataKey::ZkCacheSlot(slot);
    env.storage().persistent().set(&slot_key, &key);
    env.storage()
        .persistent()
        .extend_ttl(&slot_key, LEDGER_BUMP, LEDGER_BUMP);

    let entry_key = DataKey::ZkCacheEntry(key);
    env.storage().persistent().set(&entry_key, &verdict);
    env.storage()
        .persistent()
        .extend_ttl(&entry_key, LEDGER_BUMP, LEDGER_BUMP);
}

/// Invalidate every cached verdict for `commitment` (#1100). Called whenever
/// an IP's commitment hash changes, so verdicts computed against the old
/// value can no longer be returned.
pub fn invalidate(env: &Env, commitment: &BytesN<32>) {
    let epoch = epoch_of(env, commitment).wrapping_add(1);
    let epoch_key = DataKey::ZkCacheEpoch(commitment.clone());
    env.storage().persistent().set(&epoch_key, &epoch);
    env.storage()
        .persistent()
        .extend_ttl(&epoch_key, LEDGER_BUMP, LEDGER_BUMP);

    let mut stats = stats(env);
    stats.invalidations += 1;
    save_stats(env, &stats);

    env.events().publish(
        (Symbol::new(env, "zk_cache_inv"),),
        (commitment.clone(), epoch),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_rate_is_zero_before_any_lookup() {
        assert_eq!(ZkCacheStats::default().hit_rate_bps(), 0);
    }

    #[test]
    fn hit_rate_in_basis_points() {
        let stats = ZkCacheStats {
            hits: 3,
            misses: 1,
            ..Default::default()
        };
        assert_eq!(stats.hit_rate_bps(), 7_500);
    }

    #[test]
    fn capacity_is_bounded() {
        assert!(ZK_CACHE_CAPACITY > 0);
    }
}
