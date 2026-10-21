/// Issue #973: Commitment metadata storage and search functionality.
///
/// Provides encrypted metadata storage for commitments to improve discoverability
/// without compromising privacy. Metadata is AES-GCM encrypted and indexed by tags
/// for efficient searching.

use crate::types::{
    DataKey, EncryptedField, EncryptedMetadataInfo, EncryptedMetadataRecord, IpMetadata,
    MetadataSearchResult,
};
use soroban_sdk::{Bytes, BytesN, Env, Map, Symbol, Vec};

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
        .extend_ttl(&key, LEDGER_BUMP, LEDGER_BUMP);

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
        .extend_ttl(&key, LEDGER_BUMP, LEDGER_BUMP);
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

    let key = DataKey::MetadataTagIndex(tag_hash.clone());
    let ip_ids: Vec<u64> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));

    let total_count = ip_ids.len() as u32;

    // Calculate pagination
    let start = offset as usize;
    let end = ((offset + limit) as usize).min(ip_ids.len() as usize);

    let mut result_ids = Vec::new(env);

    if start < ip_ids.len() as usize {
        for i in start..end {
            if let Some(ip_id) = ip_ids.get(i as u32) {
                result_ids.push_back(ip_id);
            }
        }
    }

    // Determine if there are more results
    let next_cursor = if end < ip_ids.len() as usize {
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
        remove_tag_indices(env, ip_id, &metadata.tag_hashes);
    }

    // Remove the metadata itself
    env.storage()
        .persistent()
        .remove(&DataKey::IpMetadata(ip_id));

    // Issue #1105: field-level encrypted metadata is removed the same way.
    if let Some(encrypted) = get_encrypted_record(env, ip_id) {
        remove_tag_indices(env, ip_id, &encrypted.tag_hashes);
    }
    env.storage()
        .persistent()
        .remove(&DataKey::EncryptedMetadata(ip_id));
}

/// Remove `ip_id` from the search index of each tag, dropping index entries left empty.
fn remove_tag_indices(env: &Env, ip_id: u64, tag_hashes: &Vec<BytesN<32>>) {
    for i in 0..tag_hashes.len() {
        if let Some(tag_hash) = tag_hashes.get(i) {
            let key = DataKey::MetadataTagIndex(tag_hash);
            if let Some(ip_ids) = env.storage().persistent().get::<_, Vec<u64>>(&key) {
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

// ── Issue #1105: Field-level encrypted metadata ─────────────────────────────

/// Maximum number of independently encrypted fields per commitment.
pub const MAX_ENCRYPTED_FIELDS: u32 = 16;

/// Maximum number of search tags per commitment.
pub const MAX_METADATA_TAGS: u32 = 32;

fn get_encrypted_record(env: &Env, ip_id: u64) -> Option<EncryptedMetadataRecord> {
    env.storage()
        .persistent()
        .get(&DataKey::EncryptedMetadata(ip_id))
}

/// Validate and store field-level encrypted metadata, then index its tags.
///
/// The ciphertext is produced client-side under the key identified by `key_id`.
/// Replacing an existing record drops its old tags from the search index first,
/// so search only reflects the current tags.
///
/// # Panics
///
/// Panics with `EncryptedMetadataInvalid` if there are no fields, too many fields,
/// too many tags, or more total ciphertext than `MAX_METADATA_BYTES`.
pub fn store_encrypted_fields(
    env: &Env,
    ip_id: u64,
    key_id: u32,
    fields: Map<Symbol, EncryptedField>,
    tag_hashes: Vec<BytesN<32>>,
) {
    if fields.is_empty() || fields.len() > MAX_ENCRYPTED_FIELDS {
        env.panic_with_error(crate::ContractError::EncryptedMetadataInvalid);
    }
    if tag_hashes.len() > MAX_METADATA_TAGS {
        env.panic_with_error(crate::ContractError::EncryptedMetadataInvalid);
    }
    let mut total_bytes: u32 = 0;
    for (_, field) in fields.iter() {
        total_bytes = total_bytes.saturating_add(field.ciphertext.len());
    }
    if total_bytes > crate::MAX_METADATA_BYTES {
        env.panic_with_error(crate::ContractError::EncryptedMetadataInvalid);
    }

    if let Some(previous) = get_encrypted_record(env, ip_id) {
        remove_tag_indices(env, ip_id, &previous.tag_hashes);
    }

    let record = EncryptedMetadataRecord {
        ip_id,
        key_id,
        fields,
        timestamp: env.ledger().timestamp(),
        tag_hashes: tag_hashes.clone(),
    };
    let key = DataKey::EncryptedMetadata(ip_id);
    env.storage().persistent().set(&key, &record);
    env.storage()
        .persistent()
        .extend_ttl(&key, 6_307_200, 6_307_200);

    for i in 0..tag_hashes.len() {
        if let Some(tag_hash) = tag_hashes.get(i) {
            index_metadata_by_tag(env, &tag_hash, ip_id);
        }
    }
}

/// Public summary of an IP's encrypted metadata: key ID, field names, timestamp.
/// Field ciphertexts are not included.
pub fn get_encrypted_info(env: &Env, ip_id: u64) -> Option<EncryptedMetadataInfo> {
    let record = get_encrypted_record(env, ip_id)?;
    Some(EncryptedMetadataInfo {
        key_id: record.key_id,
        field_names: record.fields.keys(),
        timestamp: record.timestamp,
    })
}

/// Ciphertext of a single field. The caller is responsible for the access check.
pub fn get_encrypted_field(env: &Env, ip_id: u64, field: Symbol) -> Option<EncryptedField> {
    get_encrypted_record(env, ip_id)?.fields.get(field)
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
