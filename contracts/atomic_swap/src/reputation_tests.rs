#[cfg(test)]
mod reputation_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Bytes, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, ContractError, SwapStatus};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn setup_registry(env: &Env, owner: &Address) -> (Address, u64, BytesN<32>, BytesN<32>) {
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);

        let secret = BytesN::from_array(env, &[0xAAu8; 32]);
        let blinding = BytesN::from_array(env, &[0xBBu8; 32]);

        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(secret.clone()));
        preimage.append(&Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

        let ip_id = registry.commit_ip(owner, &commitment_hash);
        (registry_id, ip_id, secret, blinding)
    }

    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    fn setup_swap_contract(env: &Env, registry_id: &Address) -> Address {
        let contract_id = env.register(AtomicSwap, ());
        AtomicSwapClient::new(env, &contract_id).initialize(registry_id);
        contract_id
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_default_reputation_is_50() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let (registry_id, _, _, _) = setup_registry(&env, &seller);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let unknown = Address::generate(&env);
        assert_eq!(client.get_reputation(&unknown), 50);
    }

    #[test]
    fn test_reputation_increases_on_successful_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        assert_eq!(client.get_reputation(&seller), 55);
        assert_eq!(client.get_reputation(&buyer), 55);
    }

    #[test]
    fn test_reputation_decreases_on_cancel_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.cancel_swap(&swap_id, &seller);

        assert_eq!(client.get_reputation(&seller), 40);
    }

    #[test]
    fn test_reputation_decreases_on_cancel_expired_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);

        // Advance time past expiry (7 days + 1)
        env.ledger().with_mut(|l| l.timestamp += 604801);

        client.cancel_expired_swap(&swap_id, &buyer);

        // Seller defaulted: seller loses 10 points
        assert_eq!(client.get_reputation(&seller), 40);
    }

    #[test]
    fn test_admin_can_configure_expired_swap_reputation_penalty() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.set_expired_swap_reputation_penalty(&seller, &7);
        client.accept_swap(&swap_id);
        env.ledger().with_mut(|l| l.timestamp += 604801);
        client.cancel_expired_swap(&swap_id, &buyer);

        assert_eq!(client.get_expired_swap_reputation_penalty(), 7);
        assert_eq!(client.get_reputation(&seller), 43);
    }

    #[test]
    fn test_reputation_clamped_at_zero() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        // Cancel 6 times to drive reputation from 50 down to 0 (50 - 6*10 = -10 → clamped 0)
        for i in 0..6u64 {
            // Need a fresh IP for each swap since active swap lock is released on cancel
            let registry = IpRegistryClient::new(&env, &registry_id);
            let secret = BytesN::from_array(&env, &[(i as u8) + 1; 32]);
            let blinding = BytesN::from_array(&env, &[(i as u8) + 0x80; 32]);
            let mut preimage = Bytes::new(&env);
            preimage.append(&Bytes::from(secret.clone()));
            preimage.append(&Bytes::from(blinding.clone()));
            let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
            let ip_id_i = registry.commit_ip(&seller, &hash);

            let swap_id = client.initiate_swap(
                &token_id, &ip_id_i, &seller, &100i128, &buyer,
                &0u32, &None, &0i128, &false,
            );
            client.cancel_swap(&swap_id, &seller);
        }

        assert_eq!(client.get_reputation(&seller), 0);
        let _ = ip_id; // suppress unused warning
    }

    #[test]
    fn test_reputation_clamped_at_100() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 10_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        // Complete 10 swaps: 50 + 10*5 = 100
        let registry = IpRegistryClient::new(&env, &registry_id);
        for i in 0..10u64 {
            let s = BytesN::from_array(&env, &[(i as u8) + 1; 32]);
            let b = BytesN::from_array(&env, &[(i as u8) + 0x80; 32]);
            let mut preimage = Bytes::new(&env);
            preimage.append(&Bytes::from(s.clone()));
            preimage.append(&Bytes::from(b.clone()));
            let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
            let ip_id_i = registry.commit_ip(&seller, &hash);

            let swap_id = client.initiate_swap(
                &token_id, &ip_id_i, &seller, &100i128, &buyer,
                &0u32, &None, &0i128, &false,
            );
            client.accept_swap(&swap_id);
            client.reveal_key(&swap_id, &seller, &s, &b);
        }

        assert_eq!(client.get_reputation(&seller), 100);
        let _ = (ip_id, secret, blinding);
    }

    #[test]
    fn test_set_reputation_multiplier_enforced() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );

        // Seller requires buyer reputation ≥ 80; buyer has default 50
        client.set_reputation_multiplier(&swap_id, &80u32);

        let result = client.try_accept_swap(&swap_id);
        assert!(
            result.is_err(),
            "accept_swap must fail when buyer reputation is below minimum"
        );
    }

    #[test]
    fn test_set_reputation_multiplier_passes_when_met() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let contract_id = setup_swap_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        // Boost buyer reputation to 55 via a prior successful swap
        let registry = IpRegistryClient::new(&env, &registry_id);
        let s2 = BytesN::from_array(&env, &[0x11u8; 32]);
        let b2 = BytesN::from_array(&env, &[0x22u8; 32]);
        let mut preimage = Bytes::new(&env);
        preimage.append(&Bytes::from(s2.clone()));
        preimage.append(&Bytes::from(b2.clone()));
        let hash2: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id2 = registry.commit_ip(&seller, &hash2);

        let swap_id2 = client.initiate_swap(
            &token_id, &ip_id2, &seller, &100i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id2);
        client.reveal_key(&swap_id2, &seller, &s2, &b2);
        // buyer reputation is now 55

        // Now initiate the real swap with min_reputation = 55
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.set_reputation_multiplier(&swap_id, &55u32);

        // Should succeed
        client.accept_swap(&swap_id);
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Accepted);
        let _ = (secret, blinding);
    }
}
