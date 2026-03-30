#[cfg(test)]
mod tests {
    use soroban_sdk::{BytesN, Env, Address};
    use soroban_sdk::testutils::Address as _;
    use crate::{IpRegistry, IpRegistryClient};

    #[test]
    fn test_commit_and_get_ip() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let commitment_hash = BytesN::from_array(&env, &[1; 32]);

        let id = client.commit_ip(&owner, &commitment_hash);

        let record = client.get_ip(&id);
        assert_eq!(record.owner, owner);
        assert_eq!(record.commitment_hash, commitment_hash);
    }

    #[test]
    fn test_multiple_ip_records() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let hash1 = BytesN::from_array(&env, &[1; 32]);
        let hash2 = BytesN::from_array(&env, &[2; 32]);

        let id1 = client.commit_ip(&owner, &hash1);
        let id2 = client.commit_ip(&owner, &hash2);

        let record1 = client.get_ip(&id1);
        let record2 = client.get_ip(&id2);
        assert_eq!(record1.owner, owner);
        assert_eq!(record2.owner, owner);

        let ip_list = client.list_ip_by_owner(&owner);
        assert_eq!(ip_list.len(), 2);
    }

    #[test]
    fn test_verify_commitment() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let secret = BytesN::from_array(&env, &[42; 32]);
        let blinding = BytesN::from_array(&env, &[7; 32]);

        // Build commitment_hash = sha256(secret || blinding)
        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

        let id = client.commit_ip(&owner, &commitment_hash);

        assert!(client.verify_commitment(&id, &secret, &blinding));

        let wrong_secret = BytesN::from_array(&env, &[99; 32]);
        assert!(!client.verify_commitment(&id, &wrong_secret, &blinding));
    }
}
