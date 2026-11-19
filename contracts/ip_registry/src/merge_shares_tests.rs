#[cfg(test)]
mod merge_shares_tests {
    use crate::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

    fn setup() -> (Env, IpRegistryClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        (env, client)
    }

    #[test]
    fn merge_gives_duplicate_owner_a_share_taken_from_primary() {
        let (env, client) = setup();
        let primary_owner = Address::generate(&env);
        let dup_owner = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[0x5Au8; 32]);
        let primary = client.commit_ip(&primary_owner, &hash, &0);
        let dup = client.commit_ip(&dup_owner, &hash, &0);

        client.merge_duplicate_commitment(&primary, &dup);

        let record = client.get_ip(&primary);
        assert!(record.co_owners.iter().any(|a| a == dup_owner));

        let shares = client.get_ownership_shares(&primary);
        assert_eq!(shares.len(), 2);
        let total: u32 = shares.iter().map(|s| s.percentage).sum();
        assert_eq!(total, 100);
        let dup_share = shares.iter().find(|s| s.address == dup_owner).unwrap();
        assert_eq!(dup_share.percentage, 50);
        let owner_share = shares.iter().find(|s| s.address == primary_owner).unwrap();
        assert_eq!(owner_share.percentage, 50);
    }

    #[test]
    fn merge_rejects_when_primary_owner_has_no_share_to_split() {
        let (env, client) = setup();
        let primary_owner = Address::generate(&env);
        let dup_owner = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[0x6Bu8; 32]);
        let primary = client.commit_ip(&primary_owner, &hash, &0);
        let dup = client.commit_ip(&dup_owner, &hash, &0);
        // Leave the primary owner with 1%, which cannot be halved.
        let carol = Address::generate(&env);
        client.add_co_owner(&primary, &carol, &99);

        assert!(client
            .try_merge_duplicate_commitment(&primary, &dup)
            .is_err());
        assert!(!client
            .get_ip(&primary)
            .co_owners
            .iter()
            .any(|a| a == dup_owner));
    }
}
