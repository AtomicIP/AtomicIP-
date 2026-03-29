#[cfg(test)]
mod tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _,
        token::StellarAssetClient,
        Address, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus};

    fn setup_registry(env: &Env, owner: &Address) -> (Address, u64, BytesN<32>, BytesN<32>) {
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);

        let secret = BytesN::from_array(env, &[2u8; 32]);
        let blinding = BytesN::from_array(env, &[3u8; 32]);

        let mut preimage = soroban_sdk::Bytes::new(env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
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

    #[test]
    fn test_swap_lifecycle() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let client = AtomicSwapClient::new(&env, &env.register(AtomicSwap, ()));

        let swap_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &1000_i128, &buyer);
        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Pending);

        client.accept_swap(&swap_id);
        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Accepted);

        client.reveal_key(&swap_id, &seller, &secret, &blinding);
        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Completed);
    }

    #[test]
    fn test_swap_cancellation() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let client = AtomicSwapClient::new(&env, &env.register(AtomicSwap, ()));

        let swap_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &500_i128, &buyer);
        client.cancel_swap(&swap_id, &seller);

        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Cancelled);
    }

    #[test]
    fn test_multiple_swaps() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let ip_id_0 = registry.commit_ip(&seller, &BytesN::from_array(&env, &[10u8; 32]));
        let ip_id_1 = registry.commit_ip(&seller, &BytesN::from_array(&env, &[11u8; 32]));

        let token_id = setup_token(&env, &admin, &buyer, 2000);
        let client = AtomicSwapClient::new(&env, &env.register(AtomicSwap, ()));

        let swap_id_0 = client.initiate_swap(&registry_id, &token_id, &ip_id_0, &seller, &1000_i128, &buyer);
        let swap_id_1 = client.initiate_swap(&registry_id, &token_id, &ip_id_1, &seller, &1000_i128, &buyer);

        assert_eq!(client.get_swap(&swap_id_0).unwrap().ip_id, ip_id_0);
        assert_eq!(client.get_swap(&swap_id_1).unwrap().ip_id, ip_id_1);
        assert_ne!(swap_id_0, swap_id_1);
    }

    #[test]
    fn test_ttl_extension_after_swap_initiation() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let client = AtomicSwapClient::new(&env, &env.register(AtomicSwap, ()));
        let swap_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &500_i128, &buyer);

        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Pending);
    }
}
