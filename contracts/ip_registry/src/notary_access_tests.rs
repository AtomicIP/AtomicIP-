//! Regression tests for the notary and access-grant hardening issues:
//!   - #1148: notarization is write-once
//!   - #1149: notary signatures are domain-separated
//!   - #1150: invalid access levels return `InvalidAccessLevel`
//!   - #1151: access grants can expire and self-grants are rejected

#[cfg(test)]
mod notary_access_tests {
    use ed25519_dalek::{Signer, SigningKey};
    use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env};

    use crate::{IpRegistry, IpRegistryClient};

    /// Fixed notary key used by every test that signs a notarization.
    fn notary_key() -> SigningKey {
        SigningKey::from_bytes(&[42u8; 32])
    }

    /// Register the contract, initialize it and configure the notary key.
    /// Returns the contract address and the admin/owner address.
    fn setup(env: &Env) -> (Address, Address) {
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let owner = Address::generate(env);
        let client = IpRegistryClient::new(env, &contract_id);
        client.initialize(&owner);
        let public_key = BytesN::from_array(env, notary_key().verifying_key().as_bytes());
        client.set_notary_public_key(&public_key);
        (contract_id, owner)
    }

    /// Sign the current notarization message bound to `contract`.
    fn sign_for(env: &Env, contract: &Address, ip_id: u64, timestamp: u64) -> Bytes {
        let msg = crate::notary_signing_message(env, contract, ip_id, timestamp);
        let mut buf = [0u8; 256];
        let len = msg.len() as usize;
        msg.copy_into_slice(&mut buf[..len]);
        let sig = notary_key().sign(&buf[..len]);
        Bytes::from_slice(env, &sig.to_bytes())
    }

    /// Sign the pre-#1149 untagged message `ip_id || timestamp`.
    fn sign_legacy(env: &Env, ip_id: u64, timestamp: u64) -> Bytes {
        let mut msg = [0u8; 16];
        msg[..8].copy_from_slice(&ip_id.to_be_bytes());
        msg[8..].copy_from_slice(&timestamp.to_be_bytes());
        let sig = notary_key().sign(&msg);
        Bytes::from_slice(env, &sig.to_bytes())
    }

    // ── #1148: write-once notarization ───────────────────────────────────────

    #[test]
    #[should_panic(expected = "Error(Contract, #60)")]
    fn notarize_twice_is_rejected() {
        let env = Env::default();
        let (contract_id, owner) = setup(&env);
        let client = IpRegistryClient::new(&env, &contract_id);
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[7u8; 32]), &0u32);
        let timestamp = client.get_ip(&ip_id).timestamp;

        let first = sign_for(&env, &contract_id, ip_id, timestamp);
        client.notarize_ip_timestamp(&ip_id, &first);

        // A second, valid signature must be refused, not overwrite the first.
        let second = sign_for(&env, &contract_id, ip_id, timestamp);
        client.notarize_ip_timestamp(&ip_id, &second);
    }

    // ── #1149: domain-separated notary signatures ────────────────────────────

    #[test]
    fn notarize_with_current_format_succeeds() {
        let env = Env::default();
        let (contract_id, owner) = setup(&env);
        let client = IpRegistryClient::new(&env, &contract_id);
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[9u8; 32]), &0u32);
        let timestamp = client.get_ip(&ip_id).timestamp;

        let sig = sign_for(&env, &contract_id, ip_id, timestamp);
        client.notarize_ip_timestamp(&ip_id, &sig);
        assert_eq!(client.get_ip_notary_signature(&ip_id), Some(sig));
    }

    #[test]
    #[should_panic]
    fn signature_from_another_deployment_is_rejected() {
        let env = Env::default();
        // Two deployments share the same notary key.
        let (contract_a, owner_a) = setup(&env);
        let (contract_b, _owner_b) = setup(&env);
        let client_a = IpRegistryClient::new(&env, &contract_a);
        let ip_id = client_a.commit_ip(&owner_a, &BytesN::from_array(&env, &[10u8; 32]), &0u32);
        let timestamp = client_a.get_ip(&ip_id).timestamp;

        // Signed for deployment B, replayed against deployment A.
        let replayed = sign_for(&env, &contract_b, ip_id, timestamp);
        client_a.notarize_ip_timestamp(&ip_id, &replayed);
    }

    #[test]
    #[should_panic]
    fn legacy_untagged_signature_is_rejected() {
        let env = Env::default();
        let (contract_id, owner) = setup(&env);
        let client = IpRegistryClient::new(&env, &contract_id);
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[11u8; 32]), &0u32);
        let timestamp = client.get_ip(&ip_id).timestamp;

        let legacy = sign_legacy(&env, ip_id, timestamp);
        client.notarize_ip_timestamp(&ip_id, &legacy);
    }

    #[test]
    fn first_notarization_is_kept() {
        let env = Env::default();
        let (contract_id, owner) = setup(&env);
        let client = IpRegistryClient::new(&env, &contract_id);
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[8u8; 32]), &0u32);
        let timestamp = client.get_ip(&ip_id).timestamp;

        let first = sign_for(&env, &contract_id, ip_id, timestamp);
        client.notarize_ip_timestamp(&ip_id, &first);

        assert_eq!(client.get_ip_notary_signature(&ip_id), Some(first));
    }

    // ── #1150: dedicated error for invalid access levels ─────────────────────

    #[test]
    #[should_panic(expected = "Error(Contract, #61)")]
    fn grant_level_zero_returns_invalid_access_level() {
        let env = Env::default();
        let (contract_id, owner) = setup(&env);
        let client = IpRegistryClient::new(&env, &contract_id);
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[12u8; 32]), &0u32);
        client.grant_ip_access(&ip_id, &Address::generate(&env), &0u32);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #61)")]
    fn grant_level_above_three_returns_invalid_access_level() {
        let env = Env::default();
        let (contract_id, owner) = setup(&env);
        let client = IpRegistryClient::new(&env, &contract_id);
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[13u8; 32]), &0u32);
        client.grant_ip_access(&ip_id, &Address::generate(&env), &4u32);
    }

    #[test]
    fn grant_each_valid_level_is_accepted() {
        let env = Env::default();
        let (contract_id, owner) = setup(&env);
        let client = IpRegistryClient::new(&env, &contract_id);
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[14u8; 32]), &0u32);
        let grantee = Address::generate(&env);
        for level in 1u32..=3 {
            client.grant_ip_access(&ip_id, &grantee, &level);
            assert!(client.check_ip_access(&ip_id, &grantee, &level));
        }
    }

    // ── #1151: grant expiry and self-grant guard ─────────────────────────────

    #[test]
    #[should_panic(expected = "Error(Contract, #62)")]
    fn owner_cannot_grant_to_self() {
        let env = Env::default();
        let (contract_id, owner) = setup(&env);
        let client = IpRegistryClient::new(&env, &contract_id);
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[15u8; 32]), &0u32);
        client.grant_ip_access(&ip_id, &owner, &1u32);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #11)")]
    fn expiry_in_the_past_is_rejected() {
        let env = Env::default();
        let (contract_id, owner) = setup(&env);
        let client = IpRegistryClient::new(&env, &contract_id);
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[16u8; 32]), &0u32);
        env.ledger().set_timestamp(1_000);
        client.grant_ip_access_with_expiry(
            &ip_id,
            &Address::generate(&env),
            &1u32,
            &Some(500u64),
        );
    }

    #[test]
    fn expired_grant_is_not_honoured() {
        let env = Env::default();
        let (contract_id, owner) = setup(&env);
        let client = IpRegistryClient::new(&env, &contract_id);
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[17u8; 32]), &0u32);
        let grantee = Address::generate(&env);

        env.ledger().set_timestamp(1_000);
        client.grant_ip_access_with_expiry(&ip_id, &grantee, &3u32, &Some(2_000u64));
        assert!(client.check_ip_access(&ip_id, &grantee, &3u32));

        // Boundary: at exactly `expires_at` the grant is already expired.
        env.ledger().set_timestamp(2_000);
        assert!(!client.check_ip_access(&ip_id, &grantee, &1u32));
    }

    #[test]
    fn grant_without_expiry_never_lapses() {
        let env = Env::default();
        let (contract_id, owner) = setup(&env);
        let client = IpRegistryClient::new(&env, &contract_id);
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[18u8; 32]), &0u32);
        let grantee = Address::generate(&env);

        client.grant_ip_access(&ip_id, &grantee, &2u32);
        env.ledger().set_timestamp(u32::MAX as u64 * 10);
        assert!(client.check_ip_access(&ip_id, &grantee, &2u32));
    }

    #[test]
    fn stored_grant_records_its_expiry() {
        let env = Env::default();
        let (contract_id, owner) = setup(&env);
        let client = IpRegistryClient::new(&env, &contract_id);
        let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[19u8; 32]), &0u32);
        let grantee = Address::generate(&env);

        env.ledger().set_timestamp(1_000);
        client.grant_ip_access_with_expiry(&ip_id, &grantee, &1u32, &Some(5_000u64));
        let grants = client.get_ip_access_grants(&ip_id);
        assert_eq!(grants.get(0).unwrap().expires_at, Some(5_000u64));
    }
}

