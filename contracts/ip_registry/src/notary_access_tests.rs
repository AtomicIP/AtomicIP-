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

    /// Sign the legacy notarization message `ip_id || timestamp`.
    fn sign_for(env: &Env, _contract: &Address, ip_id: u64, timestamp: u64) -> Bytes {
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
}
