# Migration Notes

## NextId moved to persistent storage

**Affects:** `atomic_swap`, `ip_registry`

**Problem:** `DataKey::NextId` was stored in instance storage in both contracts.
Instance storage is wiped on contract upgrade, which resets the ID counter to 0
and causes new records to collide with existing ones.

**Fix:** `NextId` is now read from and written to persistent storage with a TTL
extension of 50 000 ledgers on every write.

**On-chain migration (existing deployments):**
If you are upgrading a contract that already has records, the old `NextId` value
in instance storage will be lost. Before upgrading, read the current counter value
and write it to persistent storage via a one-time migration transaction, or set the
initial persistent value to a safe high-water mark that is above all existing IDs.

No schema change is required — `DataKey::NextId` is the same enum variant; only
the storage tier changes.
