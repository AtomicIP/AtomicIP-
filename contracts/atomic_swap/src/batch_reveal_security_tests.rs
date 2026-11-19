#[cfg(test)]
mod batch_reveal_security_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _,
        token::{StellarAssetClient, TokenClient},
        Address, Bytes, BytesN, Env, Vec,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn setup_registry(env: &Env) -> Address {
        let registry_id = env.register(IpRegistry, ());
        let _ = IpRegistryClient::new(env, &registry_id);
        registry_id
    }

    fn commit_ip(
        env: &Env,
        registry_id: &Address,
        owner: &Address,
        seed: u8,
    ) -> (u64, BytesN<32>, BytesN<32>) {
        let registry = IpRegistryClient::new(env, registry_id);
        let secret = BytesN::from_array(env, &[seed; 32]);
        let blinding = BytesN::from_array(env, &[seed.wrapping_add(0x80); 32]);
        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(secret.clone()));
        preimage.append(&Bytes::from(blinding.clone()));
        let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(owner, &hash, &0u32);
        (ip_id, secret, blinding)
    }

    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let sac = StellarAssetClient::new(env, &token_id);
        sac.mint(recipient, &amount);
        // Establish a balance entry for the hardcoded protocol treasury so the
        // protocol-fee transfer in batch_reveal_keys succeeds in the test environment.
        let treasury = Address::from_string(&soroban_sdk::String::from_str(
            env,
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        ));
        sac.mint(&treasury, &0);
        token_id
    }

    fn setup_contract(env: &Env, registry_id: &Address) -> Address {
        let contract_id = env.register(AtomicSwap, ());
        AtomicSwapClient::new(env, &contract_id).initialize(registry_id);
        contract_id
    }

    // ── #1339: duplicate swap IDs in batch_reveal_keys ───────────────────────

    #[test]
    fn test_batch_reveal_duplicate_swap_id_rejected_without_payout() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let (ip1, s1, b1) = commit_ip(&env, &registry_id, &seller, 0x01);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);
        let mut prices = Vec::new(&env);
        prices.push_back(1000i128);

        let swap_ids =
            client.batch_initiate_swap(&token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None);
        let swap_id = swap_ids.get(0).unwrap();

        let mut accept_ids = Vec::new(&env);
        accept_ids.push_back(swap_id);
        client.batch_accept_swaps(&accept_ids, &buyer);

        let token = TokenClient::new(&env, &token_id);
        let seller_before = token.balance(&seller);
        let contract_before = token.balance(&contract_id);

        // The same swap ID appears twice. Without the guard, the second iteration
        // would transfer the seller amount again from the shared contract balance.
        let mut dup_ids = Vec::new(&env);
        dup_ids.push_back(swap_id);
        dup_ids.push_back(swap_id);
        let mut secrets = Vec::new(&env);
        secrets.push_back(s1.clone());
        secrets.push_back(s1);
        let mut blindings = Vec::new(&env);
        blindings.push_back(b1.clone());
        blindings.push_back(b1);

        let result = client.try_batch_reveal_keys(&dup_ids, &secrets, &blindings, &seller);
        assert!(result.is_err(), "duplicate swap ID must be rejected");

        assert_eq!(
            token.balance(&seller),
            seller_before,
            "seller must not be paid by a rejected batch"
        );
        assert_eq!(
            token.balance(&contract_id),
            contract_before,
            "contract balance must be unchanged by a rejected batch"
        );
        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Accepted,
            "swap must remain Accepted after a rejected batch"
        );
    }
}
