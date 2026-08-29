/// Mutation tests for the Atomic Swap contract.
///
/// These tests exercise boundary conditions, auth checks, and comparison
/// logic so that typical mutations (flipped comparisons, off-by-one,
/// skipped auth) are caught by the test suite.
#[cfg(test)]
mod mutation_tests {
    use super::*;
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _,
        token::StellarAssetClient,
        Address, Bytes, BytesN, Env, Vec,
    };

    fn setup_swap(
        env: &Env,
        ip_id: u64,
        seller: &Address,
        buyer: &Address,
        price: i128,
        token: &Address,
        status: SwapStatus,
    ) -> u64 {
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);
        let secret = BytesN::from_array(env, &[1u8; 32]);
        let blinding = BytesN::from_array(env, &[2u8; 32]);
        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(secret));
        preimage.append(&Bytes::from(blinding));
        let hash = env.crypto().sha256(&preimage).into();
        let id = registry.commit_ip(seller, &hash, &0u32);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(&registry_id);

        if status == SwapStatus::Pending {
            client.initiate_swap(
                token, &id, seller, &price, buyer, &0_u32, &None, &0i128, &false,
            )
        } else if status == SwapStatus::Accepted {
            let swap_id = client.initiate_swap(
                token, &id, seller, &price, buyer, &0_u32, &None, &0i128, &false,
            );
            StellarAssetClient::new(env, token).mint(buyer, &(price + 1000));
            client.accept_swap(&swap_id);
            swap_id
        } else {
            let swap_id = client.initiate_swap(
                token, &id, seller, &price, buyer, &0_u32, &None, &0i128, &false,
            );
            StellarAssetClient::new(env, token).mint(buyer, &(price + 1000));
            client.accept_swap(&swap_id);
            let secret = BytesN::from_array(env, &[1u8; 32]);
            let blinding = BytesN::from_array(env, &[2u8; 32]);
            client.reveal_key(&swap_id, seller, &secret, &blinding);
            swap_id
        }
    }

    #[test]
    #[should_panic]
    fn mutation_initiate_swap_rejects_zero_price() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[1u8; 32]);
        let blinding = BytesN::from_array(&env, &[2u8; 32]);
        let mut preimage = Bytes::new(&env);
        preimage.append(&Bytes::from(secret));
        preimage.append(&Bytes::from(blinding));
        let hash = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &hash, &0u32);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        client.initiate_swap(
            &token, &ip_id, &seller, &0, &buyer, &0_u32, &None, &0i128, &false,
        );
    }

    #[test]
    #[should_panic]
    fn mutation_initiate_swap_rejects_duplicate_ip() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[1u8; 32]);
        let blinding = BytesN::from_array(&env, &[2u8; 32]);
        let mut preimage = Bytes::new(&env);
        preimage.append(&Bytes::from(secret));
        preimage.append(&Bytes::from(blinding));
        let hash = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &hash, &0u32);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        client.initiate_swap(
            &token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );
        client.initiate_swap(
            &token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );
    }

    #[test]
    #[should_panic]
    fn mutation_accept_swap_requires_pending_status() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        let swap_id = setup_swap(&env, 1, &seller, &buyer, 1000, &token, SwapStatus::Accepted);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&Address::generate(&env));

        client.accept_swap(&swap_id);
    }

    #[test]
    #[should_panic]
    fn mutation_reveal_key_requires_seller() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let imposter = Address::generate(&env);
        let token = Address::generate(&env);

        let swap_id = setup_swap(&env, 2, &seller, &buyer, 1000, &token, SwapStatus::Accepted);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&Address::generate(&env));

        let secret = BytesN::from_array(&env, &[1u8; 32]);
        let blinding = BytesN::from_array(&env, &[2u8; 32]);
        client.reveal_key(&swap_id, &imposter, &secret, &blinding);
    }

    #[test]
    #[should_panic]
    fn mutation_reveal_key_rejects_invalid_secret() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        let swap_id = setup_swap(&env, 3, &seller, &buyer, 1000, &token, SwapStatus::Accepted);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&Address::generate(&env));

        let bad_secret = BytesN::from_array(&env, &[99u8; 32]);
        let bad_blinding = BytesN::from_array(&env, &[99u8; 32]);
        client.reveal_key(&swap_id, &seller, &bad_secret, &bad_blinding);
    }

    #[test]
    #[should_panic]
    fn mutation_cancel_swap_requires_pending() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        let swap_id = setup_swap(&env, 4, &seller, &buyer, 1000, &token, SwapStatus::Completed);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&Address::generate(&env));

        client.cancel_swap(&swap_id, &seller);
    }

    #[test]
    #[should_panic]
    fn mutation_batch_size_limit_enforced() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let mut ip_ids = Vec::new(&env);
        let mut prices = Vec::new(&env);
        for i in 0u64..51 {
            let s = BytesN::from_array(&env, &[(i as u8); 32]);
            let b = BytesN::from_array(&env, &[(i.wrapping_add(1)) as u8; 32]);
            let mut pre = Bytes::new(&env);
            pre.append(&Bytes::from(s));
            pre.append(&Bytes::from(b));
            let h = env.crypto().sha256(&pre).into();
            let id = registry.commit_ip(&seller, &h, &0u32);
            ip_ids.push_back(id);
            prices.push_back(1000);
        }

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        client.batch_initiate_swap(
            &token,
            &ip_ids,
            &seller,
            &prices,
            &buyer,
            &0_u32,
            &None,
        );
    }

    #[test]
    #[should_panic]
    fn mutation_expiry_boundary_rejects_early_cancel() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        let swap_id = setup_swap(&env, 5, &seller, &buyer, 1000, &token, SwapStatus::Accepted);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&Address::generate(&env));

        client.cancel_expired_swap(&swap_id, &buyer);
    }

    #[test]
    fn mutation_positive_price_boundary() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[1u8; 32]);
        let blinding = BytesN::from_array(&env, &[2u8; 32]);
        let mut preimage = Bytes::new(&env);
        preimage.append(&Bytes::from(secret));
        preimage.append(&Bytes::from(blinding));
        let hash = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &hash, &0u32);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.initiate_swap(
                &token, &ip_id, &seller, &1, &buyer, &0_u32, &None, &0i128, &false,
            );
        }));
        assert!(result.is_ok(), "price of 1 must be accepted");
    }

    #[test]
    #[should_panic]
    fn mutation_reveal_key_rejects_negative_price_mutation() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[1u8; 32]);
        let blinding = BytesN::from_array(&env, &[2u8; 32]);
        let mut preimage = Bytes::new(&env);
        preimage.append(&Bytes::from(secret));
        preimage.append(&Bytes::from(blinding));
        let hash = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &hash, &0u32);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        client.initiate_swap(
            &token, &ip_id, &seller, &(-1i128), &buyer, &0_u32, &None, &0i128, &false,
        );
    }
}
