#[cfg(test)]
mod referrer_validation_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _, token::StellarAssetClient, Address, Bytes, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient};

    const PRICE: i128 = 1_000;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn setup_registry(env: &Env, owner: &Address) -> (Address, u64) {
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);

        let secret = BytesN::from_array(env, &[0xAAu8; 32]);
        let blinding = BytesN::from_array(env, &[0xBBu8; 32]);

        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(secret.clone()));
        preimage.append(&Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

        let ip_id = registry.commit_ip(owner, &commitment_hash, &0u32);
        (registry_id, ip_id)
    }

    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    fn setup_contract(env: &Env, registry_id: &Address) -> AtomicSwapClient {
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(registry_id);
        client
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_referrer_equal_to_buyer_is_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, PRICE);
        let client = setup_contract(&env, &registry_id);

        let result = client.try_initiate_swap(
            &token_id,
            &ip_id,
            &seller,
            &PRICE,
            &buyer,
            &0u32,
            &Some(buyer.clone()),
            &0i128,
            &false,
        );
        assert!(result.is_err());
        assert!(client.get_swaps_by_ip(&ip_id).is_none());
    }

    #[test]
    fn test_referrer_equal_to_seller_is_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, PRICE);
        let client = setup_contract(&env, &registry_id);

        let result = client.try_initiate_swap(
            &token_id,
            &ip_id,
            &seller,
            &PRICE,
            &buyer,
            &0u32,
            &Some(seller.clone()),
            &0i128,
            &false,
        );
        assert!(result.is_err());
        assert!(client.get_swaps_by_ip(&ip_id).is_none());
    }

    #[test]
    fn test_third_party_referrer_is_accepted() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let referrer = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, PRICE);
        let client = setup_contract(&env, &registry_id);

        let swap_id = client.initiate_swap(
            &token_id,
            &ip_id,
            &seller,
            &PRICE,
            &buyer,
            &0u32,
            &Some(referrer.clone()),
            &0i128,
            &false,
        );

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.referrer, Some(referrer));
    }
}
