#[cfg(test)]
mod batch_approval_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _,
        token::StellarAssetClient,
        Address, Bytes, BytesN, Env, Vec,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus, ContractError, Error};

    fn setup_registry(env: &Env, owner: &Address) -> Address {
        let registry_id = env.register(IpRegistry, ());
        let _ = IpRegistryClient::new(env, &registry_id);
        registry_id
    }

    fn commit_ip(env: &Env, registry_id: &Address, owner: &Address, seed: u8) -> (u64, BytesN<32>, BytesN<32>) {
        let registry = IpRegistryClient::new(env, registry_id);
        let secret = BytesN::from_array(env, &[seed; 32]);
        let blinding = BytesN::from_array(env, &[seed.wrapping_add(0x80); 32]);
        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(secret.clone()));
        preimage.append(&Bytes::from(blinding.clone()));
        let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(owner, &hash);
        (ip_id, secret, blinding)
    }

    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    fn setup_contract(env: &Env, registry_id: &Address) -> Address {
        let contract_id = env.register(AtomicSwap, ());
        AtomicSwapClient::new(env, &contract_id).initialize(registry_id);
        contract_id
    }

    // ── #504: Batch Swap Approval Tests ───────────────────────────────────────

    #[test]
    fn test_batch_approve_single_approval() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let approver1 = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, _, _) = commit_ip(&env, &registry_id, &seller, 0x01);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);

        let mut prices = Vec::new(&env);
        prices.push_back(1000i128);

        let swap_ids = client.batch_initiate_swap(
            &token_id, &ip_ids, &seller, &prices, &buyer, &1u32, &None,
        );

        let swap_id = swap_ids.get(0).unwrap();
        
        // Approve with required approvals = 1
        client.approve_swap(&swap_id, &approver1);

        let approvals = client.get_swap_approvals(&swap_id);
        assert_eq!(approvals.len(), 1);
        assert_eq!(approvals.get(0).unwrap(), approver1);
    }

    #[test]
    fn test_batch_approve_multiple_approvers() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let approver1 = Address::generate(&env);
        let approver2 = Address::generate(&env);
        let approver3 = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, _, _) = commit_ip(&env, &registry_id, &seller, 0x01);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);

        let mut prices = Vec::new(&env);
        prices.push_back(1000i128);

        let swap_ids = client.batch_initiate_swap(
            &token_id, &ip_ids, &seller, &prices, &buyer, &3u32, &None,
        );

        let swap_id = swap_ids.get(0).unwrap();

        client.approve_swap(&swap_id, &approver1);
        client.approve_swap(&swap_id, &approver2);
        client.approve_swap(&swap_id, &approver3);

        let approvals = client.get_swap_approvals(&swap_id);
        assert_eq!(approvals.len(), 3);
        assert_eq!(approvals.get(0).unwrap(), approver1);
        assert_eq!(approvals.get(1).unwrap(), approver2);
        assert_eq!(approvals.get(2).unwrap(), approver3);
    }

    #[test]
    fn test_batch_approve_prevents_duplicate() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let approver = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, _, _) = commit_ip(&env, &registry_id, &seller, 0x01);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);

        let mut prices = Vec::new(&env);
        prices.push_back(1000i128);

        let swap_ids = client.batch_initiate_swap(
            &token_id, &ip_ids, &seller, &prices, &buyer, &2u32, &None,
        );

        let swap_id = swap_ids.get(0).unwrap();

        client.approve_swap(&swap_id, &approver);
        
        // Second approval from same approver should fail
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.approve_swap(&swap_id, &approver);
        }));
        
        assert!(result.is_err());
    }

    #[test]
    fn test_batch_approve_only_pending() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let approver = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, _, _) = commit_ip(&env, &registry_id, &seller, 0x01);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);

        let mut prices = Vec::new(&env);
        prices.push_back(1000i128);

        let swap_ids = client.batch_initiate_swap(
            &token_id, &ip_ids, &seller, &prices, &buyer, &1u32, &None,
        );

        let swap_id = swap_ids.get(0).unwrap();

        // Accept the swap - now it's Accepted, not Pending
        let mut ids = Vec::new(&env);
        ids.push_back(swap_id);
        client.batch_accept_swaps(&ids, &buyer);

        // Try to approve an Accepted swap - should fail
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.approve_swap(&swap_id, &approver);
        }));
        
        assert!(result.is_err());
    }

    #[test]
    fn test_batch_approve_multiple_swaps() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let approver = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, _, _) = commit_ip(&env, &registry_id, &seller, 0x01);
        let (ip2, _, _) = commit_ip(&env, &registry_id, &seller, 0x02);
        let (ip3, _, _) = commit_ip(&env, &registry_id, &seller, 0x03);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);
        ip_ids.push_back(ip2);
        ip_ids.push_back(ip3);

        let mut prices = Vec::new(&env);
        prices.push_back(1000i128);
        prices.push_back(2000i128);
        prices.push_back(3000i128);

        let swap_ids = client.batch_initiate_swap(
            &token_id, &ip_ids, &seller, &prices, &buyer, &1u32, &None,
        );

        // Approve each swap
        for i in 0..swap_ids.len() {
            let swap_id = swap_ids.get(i).unwrap();
            client.approve_swap(&swap_id, &approver);
        }

        // Verify each swap has exactly 1 approval
        for i in 0..swap_ids.len() {
            let swap_id = swap_ids.get(i).unwrap();
            let approvals = client.get_swap_approvals(&swap_id);
            assert_eq!(approvals.len(), 1);
            assert_eq!(approvals.get(0).unwrap(), approver);
        }
    }

    #[test]
    fn test_batch_approve_clears_on_completion() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let approver = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, secret, blinding) = commit_ip(&env, &registry_id, &seller, 0x01);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);

        let mut prices = Vec::new(&env);
        prices.push_back(1000i128);

        let swap_ids = client.batch_initiate_swap(
            &token_id, &ip_ids, &seller, &prices, &buyer, &1u32, &None,
        );

        let swap_id = swap_ids.get(0).unwrap();

        client.approve_swap(&swap_id, &approver);

        let mut ids = Vec::new(&env);
        ids.push_back(swap_id);
        client.batch_accept_swaps(&ids, &buyer);

        let mut secrets = Vec::new(&env);
        secrets.push_back(secret);

        let mut blindings = Vec::new(&env);
        blindings.push_back(blinding);

        // Reveal keys - completes the swap
        client.batch_reveal_keys(&ids, &secrets, &blindings, &seller);

        // Approvals should still exist (not cleared on completion)
        let approvals = client.get_swap_approvals(&swap_id);
        assert_eq!(approvals.len(), 1);
    }

    #[test]
    fn test_batch_approve_get_approvals_empty() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, _, _) = commit_ip(&env, &registry_id, &seller, 0x01);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);

        let mut prices = Vec::new(&env);
        prices.push_back(1000i128);

        let swap_ids = client.batch_initiate_swap(
            &token_id, &ip_ids, &seller, &prices, &buyer, &1u32, &None,
        );

        let swap_id = swap_ids.get(0).unwrap();

        // No approvals yet
        let approvals = client.get_swap_approvals(&swap_id);
        assert_eq!(approvals.len(), 0);
    }
}
