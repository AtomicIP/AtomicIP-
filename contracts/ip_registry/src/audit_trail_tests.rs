#[cfg(test)]
mod audit_trail_tests {
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

    /// Actions recorded in the audit trail for `ip_id`, in order.
    fn actions(registry: &IpRegistryClient, ip_id: u64) -> std::vec::Vec<Symbol> {
        let trail = registry.get_ip_audit_trail(&ip_id);
        let mut out = std::vec::Vec::new();
        for entry in trail.iter() {
            out.push(entry.action);
        }
        out
    }

    fn has_action(registry: &IpRegistryClient, ip_id: u64, action: Symbol) -> bool {
        actions(registry, ip_id).contains(&action)
    }

    // ── Commitment operations ─────────────────────────────────────────────────

    #[test]
    fn commit_is_recorded_with_actor() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &hash(&env, b"audit-commit"), &0);

        let trail = registry.get_ip_audit_trail(&ip_id);
        assert_eq!(trail.len(), 1);
        let entry = trail.get(0).unwrap();
        assert_eq!(entry.action, symbol_short!("committed"));
        assert_eq!(entry.actor, owner);
    }

    #[test]
    fn privacy_change_is_recorded() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &hash(&env, b"audit-privacy"), &0);
        let allowed = Vec::new(&env);

        registry.set_commitment_privacy(&ip_id, &1, &allowed);
        assert!(has_action(&registry, ip_id, symbol_short!("privacy")));
    }

    #[test]
    fn archive_is_recorded() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &hash(&env, b"audit-archive"), &0);

        registry.archive_commitment(&ip_id);
        assert!(has_action(&registry, ip_id, symbol_short!("archived")));
    }

    // ── Access grants and licenses ────────────────────────────────────────────

    #[test]
    fn access_grant_and_revoke_are_recorded() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let grantee = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &hash(&env, b"audit-access"), &0);

        registry.grant_ip_access(&ip_id, &grantee, &1);
        registry.revoke_ip_access(&ip_id, &grantee);

        let recorded = actions(&registry, ip_id);
        assert!(recorded.contains(&symbol_short!("acc_grnt")));
        assert!(recorded.contains(&symbol_short!("acc_rvk")));
    }

    #[test]
    fn license_grant_and_revoke_are_recorded() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let licensee = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &hash(&env, b"audit-license"), &0);

        registry.grant_license(&ip_id, &licensee, &hash(&env, b"terms"));
        registry.revoke_license(&ip_id, &licensee);

        let recorded = actions(&registry, ip_id);
        assert!(recorded.contains(&symbol_short!("lic_grnt")));
        assert!(recorded.contains(&symbol_short!("lic_rvk")));
    }

    #[test]
    fn failed_revoke_is_not_recorded() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let stranger = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &hash(&env, b"audit-noop"), &0);
        let before = actions(&registry, ip_id).len();

        // Revoking a grant that was never made changes nothing, so nothing is logged.
        registry.revoke_ip_access(&ip_id, &stranger);
        assert_eq!(actions(&registry, ip_id).len(), before);
    }

    // ── Encrypted metadata access ─────────────────────────────────────────────

    #[test]
    fn encrypted_store_and_field_reads_record_who_read_what() {
        let (env, registry) = setup();
        registry.register_metadata_key(&1, &hash(&env, b"fp"));
        registry.rotate_metadata_key(&1);

        let owner = Address::generate(&env);
        let grantee = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &hash(&env, b"audit-md"), &0);

        let mut fields: Map<Symbol, EncryptedField> = Map::new(&env);
        fields.set(
            symbol_short!("title"),
            EncryptedField {
                ciphertext: Bytes::from_slice(&env, b"ct"),
                nonce: BytesN::from_array(&env, &[1u8; 12]),
            },
        );
        registry.store_encrypted_metadata(&ip_id, &fields, &Vec::new(&env));
        registry.grant_ip_access(&ip_id, &grantee, &1);
        registry.get_encrypted_metadata_field(&ip_id, &symbol_short!("title"), &grantee);

        let trail = registry.get_ip_audit_trail(&ip_id);
        let read = trail
            .iter()
            .find(|e| e.action == symbol_short!("md_read"))
            .expect("field read must be audited");
        assert_eq!(read.actor, grantee, "the entry records who read the field");
        assert!(has_action(&registry, ip_id, symbol_short!("md_store")));
    }

    #[test]
    fn trail_is_append_only_across_operations() {
        let (env, registry) = setup();
        let owner = Address::generate(&env);
        let ip_id = registry.commit_ip(&owner, &hash(&env, b"audit-order"), &0);
        registry.archive_commitment(&ip_id);
        registry.set_commitment_privacy(&ip_id, &1, &Vec::new(&env));

        let recorded = actions(&registry, ip_id);
        assert_eq!(recorded.first(), Some(&symbol_short!("committed")));
        assert_eq!(recorded.len(), 3);
        assert_eq!(recorded[1], symbol_short!("archived"));
        assert_eq!(recorded[2], symbol_short!("privacy"));
    }
}
