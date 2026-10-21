#[cfg(test)]
mod metadata_encryption_tests {
    use soroban_sdk::{
        symbol_short, testutils::Address as _, Address, Bytes, BytesN, Env, Map, Symbol, Vec,
    };

    use crate::{types::EncryptedField, IpRegistry, IpRegistryClient};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn setup() -> (Env, IpRegistryClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        registry.initialize(&admin);
        (env, registry)
    }

    fn hash(env: &Env, seed: &[u8]) -> BytesN<32> {
        let data = Bytes::from_slice(env, seed);
        env.crypto().sha256(&data).into()
    }

    fn field(env: &Env, ciphertext: &[u8], nonce_byte: u8) -> EncryptedField {
        EncryptedField {
            ciphertext: Bytes::from_slice(env, ciphertext),
            nonce: BytesN::from_array(env, &[nonce_byte; 12]),
        }
    }

    fn fields_map(env: &Env, entries: &[(Symbol, EncryptedField)]) -> Map<Symbol, EncryptedField> {
        let mut map = Map::new(env);
        for (name, value) in entries {
            map.set(name.clone(), value.clone());
        }
        map
    }

    fn tags(env: &Env, tag_list: &[BytesN<32>]) -> Vec<BytesN<32>> {
        let mut out = Vec::new(env);
        for tag in tag_list {
            out.push_back(tag.clone());
        }
        out
    }

    /// Register key `key_id` and make it the active key for writes.
    fn activate_key(env: &Env, registry: &IpRegistryClient, key_id: u32) {
        registry.register_metadata_key(&key_id, &hash(env, b"key-fingerprint"));
        registry.rotate_metadata_key(&key_id);
    }

    /// Commit an IP and store two encrypted fields, tagged `tag`.
    fn commit_with_encrypted_metadata(
        env: &Env,
        registry: &IpRegistryClient,
        owner: &Address,
        seed: &[u8],
        tag: &BytesN<32>,
    ) -> u64 {
        let ip_id = registry.commit_ip(owner, &hash(env, seed), &0);
        let entries = [
            (symbol_short!("title"), field(env, b"ct-title", 1)),
            (symbol_short!("descr"), field(env, b"ct-descr", 2)),
        ];
        registry.store_encrypted_metadata(
            &ip_id,
            &fields_map(env, &entries),
            &tags(env, core::slice::from_ref(tag)),
        );
        ip_id
    }

    // ── Key management ────────────────────────────────────────────────────────

    #[test]
    fn registered_key_is_not_active_until_rotated() {
        let (env, registry) = setup();
        registry.register_metadata_key(&1, &hash(&env, b"fp"));

        assert!(registry.get_active_metadata_key().is_none());
        let record = registry.get_metadata_key(&1).unwrap();
        assert_eq!(record.fingerprint, hash(&env, b"fp"));
        assert!(!record.retired);

        registry.rotate_metadata_key(&1);
        assert_eq!(registry.get_active_metadata_key(), Some(1));
    }

    #[test]
    #[should_panic]
    fn duplicate_key_registration_panics() {
        let (env, registry) = setup();
        registry.register_metadata_key(&1, &hash(&env, b"a"));
        registry.register_metadata_key(&1, &hash(&env, b"b"));
    }

    #[test]
    #[should_panic]
    fn rotating_unregistered_key_panics() {
        let (_env, registry) = setup();
        registry.rotate_metadata_key(&9);
    }

    #[test]
    #[should_panic]
    fn rotating_to_retired_key_panics() {
        let (env, registry) = setup();
        registry.register_metadata_key(&1, &hash(&env, b"a"));
        registry.retire_metadata_key(&1);
        registry.rotate_metadata_key(&1);
    }

    #[test]
    fn retiring_active_key_clears_it() {
        let (env, registry) = setup();
        activate_key(&env, &registry, 1);
        registry.retire_metadata_key(&1);

        assert!(registry.get_active_metadata_key().is_none());
        assert!(registry.get_metadata_key(&1).unwrap().retired);
    }

    #[test]
    #[should_panic]
    fn writes_fail_without_an_active_key() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &hash(&env, b"no-key"), &0);
        let entries = [(symbol_short!("title"), field(&env, b"ct", 1))];
        registry.store_encrypted_metadata(&ip_id, &fields_map(&env, &entries), &Vec::new(&env));
    }

    #[test]
    #[should_panic]
    fn writes_fail_after_active_key_is_retired() {
        let (env, registry) = setup();
        activate_key(&env, &registry, 1);
        let owner = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &hash(&env, b"retired"), &0);
        registry.retire_metadata_key(&1);

        let entries = [(symbol_short!("title"), field(&env, b"ct", 1))];
        registry.store_encrypted_metadata(&ip_id, &fields_map(&env, &entries), &Vec::new(&env));
    }

    // ── Storage and selective decryption ──────────────────────────────────────

    #[test]
    fn info_exposes_key_and_field_names_only() {
        let (env, registry) = setup();
        activate_key(&env, &registry, 1);
        let owner = Address::generate(&env);
        let tag = hash(&env, b"tag-a");
        let ip_id = commit_with_encrypted_metadata(&env, &registry, &owner, b"info", &tag);

        let info = registry.get_encrypted_metadata_info(&ip_id).unwrap();
        assert_eq!(info.key_id, 1);
        assert_eq!(info.field_names.len(), 2);
        assert!(info.field_names.contains(&symbol_short!("title")));
        assert!(info.field_names.contains(&symbol_short!("descr")));
    }

    #[test]
    fn owner_can_read_each_field() {
        let (env, registry) = setup();
        activate_key(&env, &registry, 1);
        let owner = Address::generate(&env);
        let tag = hash(&env, b"tag-b");
        let ip_id = commit_with_encrypted_metadata(&env, &registry, &owner, b"owner", &tag);

        let title = registry
            .get_encrypted_metadata_field(&ip_id, &symbol_short!("title"), &owner)
            .unwrap();
        assert_eq!(title.ciphertext, Bytes::from_slice(&env, b"ct-title"));
        assert_eq!(title.nonce, BytesN::from_array(&env, &[1u8; 12]));
    }

    #[test]
    fn granted_requester_can_read_fields() {
        let (env, registry) = setup();
        activate_key(&env, &registry, 1);
        let owner = Address::generate(&env);
        let grantee = Address::generate(&env);
        let tag = hash(&env, b"tag-c");
        let ip_id = commit_with_encrypted_metadata(&env, &registry, &owner, b"grant", &tag);
        registry.grant_ip_access(&ip_id, &grantee, &1);

        let descr = registry
            .get_encrypted_metadata_field(&ip_id, &symbol_short!("descr"), &grantee)
            .unwrap();
        assert_eq!(descr.ciphertext, Bytes::from_slice(&env, b"ct-descr"));
    }

    #[test]
    #[should_panic]
    fn ungranted_requester_is_denied() {
        let (env, registry) = setup();
        activate_key(&env, &registry, 1);
        let owner = Address::generate(&env);
        let stranger = Address::generate(&env);
        let tag = hash(&env, b"tag-d");
        let ip_id = commit_with_encrypted_metadata(&env, &registry, &owner, b"deny", &tag);

        registry.get_encrypted_metadata_field(&ip_id, &symbol_short!("title"), &stranger);
    }

    #[test]
    fn missing_field_returns_none() {
        let (env, registry) = setup();
        activate_key(&env, &registry, 1);
        let owner = Address::generate(&env);
        let tag = hash(&env, b"tag-e");
        let ip_id = commit_with_encrypted_metadata(&env, &registry, &owner, b"missing", &tag);

        let absent = registry.get_encrypted_metadata_field(&ip_id, &symbol_short!("nope"), &owner);
        assert!(absent.is_none());
    }

    #[test]
    fn info_is_none_when_nothing_stored() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &hash(&env, b"empty"), &0);
        assert!(registry.get_encrypted_metadata_info(&ip_id).is_none());
    }

    #[test]
    #[should_panic]
    fn revoked_commitment_rejects_writes() {
        let (env, registry) = setup();
        activate_key(&env, &registry, 1);
        let owner = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &hash(&env, b"revoked"), &0);
        registry.revoke_ip(&ip_id);

        let entries = [(symbol_short!("title"), field(&env, b"ct", 1))];
        registry.store_encrypted_metadata(&ip_id, &fields_map(&env, &entries), &Vec::new(&env));
    }

    // ── Search over encrypted metadata ────────────────────────────────────────

    #[test]
    fn search_finds_commitments_by_blinded_tag() {
        let (env, registry) = setup();
        activate_key(&env, &registry, 1);
        let owner = Address::generate(&env);
        let tag = hash(&env, b"music");
        let ip_id = commit_with_encrypted_metadata(&env, &registry, &owner, b"search", &tag);

        let result = registry.search_commitment_metadata(&tag, &0, &10);
        assert_eq!(result.total_count, 1);
        assert!(result.ip_ids.contains(&ip_id));
    }

    #[test]
    fn replacing_metadata_reindexes_tags() {
        let (env, registry) = setup();
        activate_key(&env, &registry, 1);
        let owner = Address::generate(&env);
        let old_tag = hash(&env, b"old-tag");
        let new_tag = hash(&env, b"new-tag");
        let ip_id = commit_with_encrypted_metadata(&env, &registry, &owner, b"reindex", &old_tag);

        let entries = [(symbol_short!("title"), field(&env, b"ct2", 3))];
        registry.store_encrypted_metadata(
            &ip_id,
            &fields_map(&env, &entries),
            &tags(&env, core::slice::from_ref(&new_tag)),
        );

        assert_eq!(registry.search_commitment_metadata(&old_tag, &0, &10).total_count, 0);
        assert_eq!(registry.search_commitment_metadata(&new_tag, &0, &10).total_count, 1);
    }

    // ── Size limits ───────────────────────────────────────────────────────────

    #[test]
    #[should_panic]
    fn too_many_fields_are_rejected() {
        let (env, registry) = setup();
        activate_key(&env, &registry, 1);
        let owner = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &hash(&env, b"many"), &0);

        let names = [
            "f00", "f01", "f02", "f03", "f04", "f05", "f06", "f07", "f08", "f09", "f10", "f11",
            "f12", "f13", "f14", "f15", "f16",
        ];
        let mut map = Map::new(&env);
        for name in names {
            map.set(Symbol::new(&env, name), field(&env, b"x", 1));
        }
        registry.store_encrypted_metadata(&ip_id, &map, &Vec::new(&env));
    }

    #[test]
    #[should_panic]
    fn total_ciphertext_over_limit_is_rejected() {
        let (env, registry) = setup();
        activate_key(&env, &registry, 1);
        let owner = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &hash(&env, b"big"), &0);

        let big = [7u8; 600];
        let entries = [
            (symbol_short!("a"), field(&env, &big, 1)),
            (symbol_short!("b"), field(&env, &big, 2)),
        ];
        registry.store_encrypted_metadata(&ip_id, &fields_map(&env, &entries), &Vec::new(&env));
    }
}
