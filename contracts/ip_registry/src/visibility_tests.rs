#[cfg(test)]
mod visibility_tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn test_default_visibility_is_private() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);

        let ip_id = client.commit_ip(&owner, &hash);
        let record = client.get_ip(&ip_id);

        assert_eq!(record.visibility, Visibility::Private);
    }

    #[test]
    fn test_set_ip_visibility_to_public() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[2u8; 32]);

        let ip_id = client.commit_ip(&owner, &hash);
        client.set_ip_visibility(&ip_id, &Visibility::Public);

        let record = client.get_ip(&ip_id);
        assert_eq!(record.visibility, Visibility::Public);
    }

    #[test]
    fn test_public_ips_appear_in_list() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let hash1 = BytesN::from_array(&env, &[3u8; 32]);
        let hash2 = BytesN::from_array(&env, &[4u8; 32]);

        let ip_id1 = client.commit_ip(&owner, &hash1);
        let ip_id2 = client.commit_ip(&owner, &hash2);

        // Initially, no public IPs
        let public_ips = client.list_public_ips();
        assert_eq!(public_ips.len(), 0);

        // Make ip_id1 public
        client.set_ip_visibility(&ip_id1, &Visibility::Public);
        let public_ips = client.list_public_ips();
        assert_eq!(public_ips.len(), 1);
        assert_eq!(public_ips.get(0).unwrap(), ip_id1);

        // Make ip_id2 public
        client.set_ip_visibility(&ip_id2, &Visibility::Public);
        let public_ips = client.list_public_ips();
        assert_eq!(public_ips.len(), 2);
    }

    #[test]
    fn test_private_ips_not_in_public_list() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[5u8; 32]);

        let ip_id = client.commit_ip(&owner, &hash);

        // IP is private by default, should not appear in public list
        let public_ips = client.list_public_ips();
        assert!(!public_ips.iter().any(|id| id == ip_id));
    }

    #[test]
    fn test_change_visibility_from_public_to_private() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[6u8; 32]);

        let ip_id = client.commit_ip(&owner, &hash);

        // Make public
        client.set_ip_visibility(&ip_id, &Visibility::Public);
        let public_ips = client.list_public_ips();
        assert!(public_ips.iter().any(|id| id == ip_id));

        // Make private again
        client.set_ip_visibility(&ip_id, &Visibility::Private);
        let public_ips = client.list_public_ips();
        assert!(!public_ips.iter().any(|id| id == ip_id));

        let record = client.get_ip(&ip_id);
        assert_eq!(record.visibility, Visibility::Private);
    }

    #[test]
    #[should_panic]
    fn test_non_owner_cannot_set_visibility() {
        let env = Env::default();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let non_owner = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[7u8; 32]);

        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &owner,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "commit_ip",
                args: (owner.clone(), hash.clone()).into_val(&env),
                sub_invokes: &[],
            },
        }]);

        let ip_id = client.commit_ip(&owner, &hash);

        // Non-owner tries to set visibility - should panic
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &non_owner,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "set_ip_visibility",
                args: (ip_id, Visibility::Public).into_val(&env),
                sub_invokes: &[],
            },
        }]);

        client.set_ip_visibility(&ip_id, &Visibility::Public);
    }

    #[test]
    fn test_owner_can_see_all_their_ips_regardless_of_visibility() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let hash1 = BytesN::from_array(&env, &[8u8; 32]);
        let hash2 = BytesN::from_array(&env, &[9u8; 32]);

        let ip_id1 = client.commit_ip(&owner, &hash1);
        let ip_id2 = client.commit_ip(&owner, &hash2);

        // Make one public, keep one private
        client.set_ip_visibility(&ip_id1, &Visibility::Public);

        // Owner should see both IPs
        let owner_ips = client.list_ip_by_owner(&owner);
        assert_eq!(owner_ips.len(), 2);
        assert!(owner_ips.iter().any(|id| id == ip_id1));
        assert!(owner_ips.iter().any(|id| id == ip_id2));
    }
}
