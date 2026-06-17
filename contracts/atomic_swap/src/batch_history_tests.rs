#[cfg(test)]
mod batch_history_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _,
        token::StellarAssetClient,
        Address, Bytes, BytesN, Env, Vec,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus, SwapHistoryEntry};

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

    // ── #503: Batch Swap History Tracking Tests ──────────────────────────────

    #[test]
    fn test_batch_history_single_swap_initiated() {
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
            &token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None,
        );

        let swap_id = swap_ids.get(0).unwrap();
        let history = client.get_swap_history(&swap_id);

        // Should have at least 1 entry for Pending status
        assert!(history.len() > 0);
        
        let first_entry = history.get(0).unwrap();
        assert_eq!(first_entry.status, SwapStatus::Pending);
    }

    #[test]
    fn test_batch_history_tracks_accepted() {
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
            &token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None,
        );

        let swap_id = swap_ids.get(0).unwrap();
        let initial_len = client.get_swap_history(&swap_id).len();

        let mut ids = Vec::new(&env);
        ids.push_back(swap_id);
        client.batch_accept_swaps(&ids, &buyer);

        let history = client.get_swap_history(&swap_id);
        assert!(history.len() > initial_len);

        // Last entry should be Accepted
        let last_entry = history.get(history.len() - 1).unwrap();
        assert_eq!(last_entry.status, SwapStatus::Accepted);
    }

    #[test]
    fn test_batch_history_tracks_completed() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
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
            &token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None,
        );

        let swap_id = swap_ids.get(0).unwrap();

        let mut ids = Vec::new(&env);
        ids.push_back(swap_id);
        client.batch_accept_swaps(&ids, &buyer);

        let mut secrets = Vec::new(&env);
        secrets.push_back(secret);

        let mut blindings = Vec::new(&env);
        blindings.push_back(blinding);

        client.batch_reveal_keys(&ids, &secrets, &blindings, &seller);

        let history = client.get_swap_history(&swap_id);
        
        // Should contain Pending -> Accepted -> Completed transitions
        assert!(history.len() >= 3);

        // Last entry should be Completed
        let last_entry = history.get(history.len() - 1).unwrap();
        assert_eq!(last_entry.status, SwapStatus::Completed);
    }

    #[test]
    fn test_batch_history_multiple_swaps() {
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
        let (ip2, _, _) = commit_ip(&env, &registry_id, &seller, 0x02);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);
        ip_ids.push_back(ip2);

        let mut prices = Vec::new(&env);
        prices.push_back(1000i128);
        prices.push_back(2000i128);

        let swap_ids = client.batch_initiate_swap(
            &token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None,
        );

        // Verify both swaps have independent history records
        for i in 0..swap_ids.len() {
            let swap_id = swap_ids.get(i).unwrap();
            let history = client.get_swap_history(&swap_id);
            assert!(history.len() > 0);
            assert_eq!(history.get(0).unwrap().status, SwapStatus::Pending);
        }
    }

    #[test]
    fn test_batch_history_tracks_cancellation() {
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
            &token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None,
        );

        let swap_id = swap_ids.get(0).unwrap();
        let initial_len = client.get_swap_history(&swap_id).len();

        client.cancel_swap(&swap_id, &seller, &soroban_sdk::Bytes::new(&env));

        let history = client.get_swap_history(&swap_id);
        assert!(history.len() > initial_len);

        // Last entry should be Cancelled
        let last_entry = history.get(history.len() - 1).unwrap();
        assert_eq!(last_entry.status, SwapStatus::Cancelled);
    }

    #[test]
    fn test_batch_history_timestamps_increase() {
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
            &token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None,
        );

        let swap_id = swap_ids.get(0).unwrap();

        let mut ids = Vec::new(&env);
        ids.push_back(swap_id);
        client.batch_accept_swaps(&ids, &buyer);

        let history = client.get_swap_history(&swap_id);

        // Verify timestamps are non-decreasing
        for i in 0..history.len() {
            if i > 0 {
                let prev_entry = history.get(i - 1).unwrap();
                let curr_entry = history.get(i).unwrap();
                assert!(curr_entry.timestamp >= prev_entry.timestamp);
            }
        }
    }

    #[test]
    fn test_batch_history_full_lifecycle() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
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
            &token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None,
        );

        let swap_id = swap_ids.get(0).unwrap();

        // Step 1: Initiate (Pending)
        let history_after_init = client.get_swap_history(&swap_id);
        assert!(history_after_init.len() > 0);

        // Step 2: Accept (Accepted)
        let mut ids = Vec::new(&env);
        ids.push_back(swap_id);
        client.batch_accept_swaps(&ids, &buyer);

        let history_after_accept = client.get_swap_history(&swap_id);
        assert_eq!(
            history_after_accept.len(),
            history_after_init.len() + 1
        );

        // Step 3: Reveal (Completed)
        let mut secrets = Vec::new(&env);
        secrets.push_back(secret);

        let mut blindings = Vec::new(&env);
        blindings.push_back(blinding);

        client.batch_reveal_keys(&ids, &secrets, &blindings, &seller);

        let history_after_reveal = client.get_swap_history(&swap_id);
        assert_eq!(
            history_after_reveal.len(),
            history_after_accept.len() + 1
        );

        // Verify final state
        let last_entry = history_after_reveal.get(history_after_reveal.len() - 1).unwrap();
        assert_eq!(last_entry.status, SwapStatus::Completed);
    }

    #[test]
    fn test_batch_history_individual_swap_independence() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, secret1, blinding1) = commit_ip(&env, &registry_id, &seller, 0x01);
        let (ip2, _, _) = commit_ip(&env, &registry_id, &seller, 0x02);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);
        ip_ids.push_back(ip2);

        let mut prices = Vec::new(&env);
        prices.push_back(1000i128);
        prices.push_back(2000i128);

        let swap_ids = client.batch_initiate_swap(
            &token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None,
        );

        let swap_id_1 = swap_ids.get(0).unwrap();
        let swap_id_2 = swap_ids.get(1).unwrap();

        // Accept only the first swap
        let mut id_1 = Vec::new(&env);
        id_1.push_back(swap_id_1);
        client.batch_accept_swaps(&id_1, &buyer);

        // Reveal only the first swap
        let mut secrets = Vec::new(&env);
        secrets.push_back(secret1);
        let mut blindings = Vec::new(&env);
        blindings.push_back(blinding1);
        client.batch_reveal_keys(&id_1, &secrets, &blindings, &seller);

        // First swap should be Completed
        let history_1 = client.get_swap_history(&swap_id_1);
        let last_1 = history_1.get(history_1.len() - 1).unwrap();
        assert_eq!(last_1.status, SwapStatus::Completed);

        // Second swap should still be Pending
        let history_2 = client.get_swap_history(&swap_id_2);
        let last_2 = history_2.get(history_2.len() - 1).unwrap();
        assert_eq!(last_2.status, SwapStatus::Pending);
    }

    #[test]
    fn test_batch_history_get_nonexistent_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env, &seller);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        // Query history for non-existent swap
        let history = client.get_swap_history(&999u64);
        
        // Should return empty vector
        assert_eq!(history.len(), 0);
    }
}
