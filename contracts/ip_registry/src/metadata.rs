/// Issue #973: Commitment metadata storage and search functionality.
///
/// Provides encrypted metadata storage for commitments to improve discoverability
/// without compromising privacy. Metadata is AES-GCM encrypted and indexed by tags
/// for efficient searching.

use crate::types::{DataKey, IpMetadata, MetadataSearchResult};
use soroban_sdk::{Bytes, BytesN, Env, Vec};

/// Maximum number of results per search page
pub const MAX_SEARCH_RESULTS: u32 = 100;

/// Store encrypted metadata for an IP commitment.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `ip_id` - The IP ID to attach metadata to
/// * `encrypted_data` - AES-GCM encrypted metadata blob
/// * `nonce` - 12-byte encryption nonce
/// * `tag_hashes` - Vec of SHA256 hashes of searchable tags
pub fn store_metadata(
    env: &Env,
    ip_id: u64,
    encrypted_data: Bytes,
    nonce: BytesN<12>,
    tag_hashes: Vec<BytesN<32>>,
) {
    // Validate metadata size (should not exceed a reasonable limit)
    if encrypted_data.len() > 2048 {
        env.panic_with_error(crate::ContractError::MetadataTooLarge);
    }

    let metadata = IpMetadata {
        ip_id,
        encrypted_data,
        nonce,
        timestamp: env.ledger().timestamp(),
        tag_hashes: tag_hashes.clone(),
    };

    // Store the metadata
    let key = DataKey::IpMetadata(ip_id);
    env.storage().persistent().set(&key, &metadata);
    env.storage()
        .persistent()
        .extend_ttl(&key, 6_307_200, 6_307_200);

    // Index by each tag for search
    for i in 0..tag_hashes.len() {
        if let Some(tag_hash) = tag_hashes.get(i) {
            index_metadata_by_tag(env, &tag_hash, ip_id);
        }
    }
}

/// Index a metadata entry by tag hash for searching.
fn index_metadata_by_tag(env: &Env, tag_hash: &BytesN<32>, ip_id: u64) {
    let key = DataKey::MetadataTagIndex(tag_hash.clone());
    let mut ip_ids: Vec<u64> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));

    // Avoid duplicate indices
    if !ip_ids.contains(&ip_id) {
        ip_ids.push_back(ip_id);
    }

    env.storage().persistent().set(&key, &ip_ids);
    env.storage()
        .persistent()
        .extend_ttl(&key, 6_307_200, 6_307_200);
}

/// Search for commitments by tag with pagination.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `tag_hash` - SHA256 hash of the tag string to search for
/// * `offset` - Starting index in results
/// * `limit` - Maximum number of results (capped at MAX_SEARCH_RESULTS)
///
/// # Returns
/// MetadataSearchResult with paginated IP IDs matching the tag
pub fn search_by_metadata(
    env: &Env,
    tag_hash: BytesN<32>,
    offset: u32,
    limit: u32,
) -> MetadataSearchResult {
    let limit = if limit > MAX_SEARCH_RESULTS {
        MAX_SEARCH_RESULTS
    } else if limit == 0 {
        MAX_SEARCH_RESULTS
    } else {
        limit
    };

    let key = DataKey::MetadataTagIndex(tag_hash);
    let ip_ids: Vec<u64> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));

    let total_count = ip_ids.len() as u32;

    // Calculate pagination
    let start = offset as usize;
    let end = ((offset + limit) as usize).min(ip_ids.len());

    let mut result_ids = Vec::new(env);

    if start < ip_ids.len() {
        for i in start..end {
            if let Some(ip_id) = ip_ids.get(i as u32) {
                result_ids.push_back(ip_id);
            }
        }
    }

    // Determine if there are more results
    let next_cursor = if end < ip_ids.len() {
        // Store cursor as hash of (tag_hash || offset)
        let mut cursor_bytes = Bytes::new(env);
        cursor_bytes.append(&Bytes::from(tag_hash.clone()));
        let offset_bytes = Bytes::from_slice(env, &(end as u32).to_le_bytes());
        cursor_bytes.append(&offset_bytes);
        Some(env.crypto().sha256(&cursor_bytes).into())
    } else {
        None
    };

    MetadataSearchResult {
        ip_ids: result_ids,
        total_count,
        offset,
        limit,
        next_cursor,
    }
}

/// Retrieve metadata for a specific IP (encrypted).
///
/// # Arguments
/// * `env` - Soroban environment
/// * `ip_id` - The IP ID to retrieve metadata for
///
/// # Returns
/// Option<IpMetadata> if metadata exists for this IP
pub fn get_metadata(env: &Env, ip_id: u64) -> Option<IpMetadata> {
    env.storage()
        .persistent()
        .get(&DataKey::IpMetadata(ip_id))
}

/// Delete metadata for an IP commitment.
/// Called when an IP is revoked or removed.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `ip_id` - The IP ID whose metadata should be deleted
pub fn delete_metadata(env: &Env, ip_id: u64) {
    if let Some(metadata) = get_metadata(env, ip_id) {
        // Remove from tag indices
        for i in 0..metadata.tag_hashes.len() {
            if let Some(tag_hash) = metadata.tag_hashes.get(i) {
                let key = DataKey::MetadataTagIndex(tag_hash);
                if let Some(mut ip_ids) = env.storage().persistent().get::<_, Vec<u64>>(&key) {
                    // Remove this IP from the index
                    let mut new_ids = Vec::new(env);
                    for j in 0..ip_ids.len() {
                        if let Some(id) = ip_ids.get(j) {
                            if id != ip_id {
                                new_ids.push_back(id);
                            }
                        }
                    }

                    if new_ids.is_empty() {
                        env.storage().persistent().remove(&key);
                    } else {
                        env.storage().persistent().set(&key, &new_ids);
                    }
                }
            }
        }
    }

    // Remove the metadata itself
    env.storage()
        .persistent()
        .remove(&DataKey::IpMetadata(ip_id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Ledger;

    #[test]
    fn test_store_and_retrieve_metadata() {
        let env = Env::default();
        env.mock_all_auths();

        let ip_id = 1u64;
        let encrypted = Bytes::from_slice(&env, b"encrypted_metadata");
        let nonce = BytesN::from_array(&env, &[0u8; 12]);

        let mut tags = Vec::new(&env);
        let tag_hash = BytesN::from_array(&env, &[1u8; 32]);
        tags.push_back(tag_hash.clone());

        store_metadata(&env, ip_id, encrypted.clone(), nonce.clone(), tags);

        let retrieved = get_metadata(&env, ip_id);
        assert!(retrieved.is_some(), "metadata should be retrievable");

        let meta = retrieved.unwrap();
        assert_eq!(meta.ip_id, ip_id);
        assert_eq!(meta.encrypted_data, encrypted);
    }

    #[test]
    fn test_search_by_metadata_pagination() {
        let env = Env::default();
        env.mock_all_auths();

        let tag_hash = BytesN::from_array(&env, &[2u8; 32]);
        let nonce = BytesN::from_array(&env, &[0u8; 12]);
        let encrypted = Bytes::from_slice(&env, b"data");

        // Store metadata for multiple IPs with the same tag
        for i in 1..=5u64 {
            let mut tags = Vec::new(&env);
            tags.push_back(tag_hash.clone());
            store_metadata(&env, i, encrypted.clone(), nonce.clone(), tags);
        }

        // Search first page
        let result = search_by_metadata(&env, tag_hash.clone(), 0, 2);
        assert_eq!(result.total_count, 5);
        assert_eq!(result.ip_ids.len(), 2);
        assert!(result.next_cursor.is_some(), "should have next page");

        // Search second page
        let result2 = search_by_metadata(&env, tag_hash.clone(), 2, 2);
        assert_eq!(result2.ip_ids.len(), 2);
        assert!(result2.next_cursor.is_some());

        // Search last page
        let result3 = search_by_metadata(&env, tag_hash.clone(), 4, 2);
        assert_eq!(result3.ip_ids.len(), 1);
        assert!(result3.next_cursor.is_none(), "no next page");
    }

    #[test]
    fn test_delete_metadata_removes_from_index() {
        let env = Env::default();
        env.mock_all_auths();

        let ip_id = 1u64;
        let tag_hash = BytesN::from_array(&env, &[3u8; 32]);
        let nonce = BytesN::from_array(&env, &[0u8; 12]);
        let encrypted = Bytes::from_slice(&env, b"data");

        let mut tags = Vec::new(&env);
        tags.push_back(tag_hash.clone());
        store_metadata(&env, ip_id, encrypted, nonce, tags);

        // Verify it's indexed
        let before = search_by_metadata(&env, tag_hash.clone(), 0, 10);
        assert_eq!(before.total_count, 1);

        // Delete the metadata
        delete_metadata(&env, ip_id);

        // Verify it's no longer indexed
        let after = search_by_metadata(&env, tag_hash.clone(), 0, 10);
        assert_eq!(after.total_count, 0);
        assert_eq!(after.ip_ids.len(), 0);

        // Verify metadata is gone
        assert!(get_metadata(&env, ip_id).is_none());
    }
}
