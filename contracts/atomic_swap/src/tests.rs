#[cfg(test)]
mod tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, DataKey, SwapStatus};

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

    fn setup_swap(env: &Env, registry_id: &Address) -> Address {
        let contract_id = env.register(AtomicSwap, ());
        AtomicSwapClient::new(env, &contract_id).initialize(registry_id);
        contract_id
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

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer);

        let ttl = env
            .storage()
            .persistent()
            .get_ttl(&DataKey::Swap(swap_id));
        assert!(ttl > 0, "TTL should be set after swap initiation");
        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Pending);
    }

    #[test]
    fn test_ttl_extension_after_swap_acceptance() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer);
        client.accept_swap(&swap_id);

        let ttl = env
            .storage()
            .persistent()
            .get_ttl(&DataKey::Swap(swap_id));
        assert!(ttl > 0, "TTL should be extended after swap acceptance");
        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Accepted);
    }

    #[test]
    fn test_ttl_extension_after_swap_completion() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer);
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        let ttl = env
            .storage()
            .persistent()
            .get_ttl(&DataKey::Swap(swap_id));
        assert!(ttl > 0, "TTL should be extended after swap completion");
        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Completed);
    }

    #[test]
    fn test_ttl_extension_after_swap_cancellation() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer);
        client.cancel_swap(&swap_id, &seller);

        let ttl = env
            .storage()
            .persistent()
            .get_ttl(&DataKey::Swap(swap_id));
        assert!(ttl > 0, "TTL should be extended after swap cancellation");
        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Cancelled);
    }

    #[test]
    fn test_multiple_ttl_extensions_during_swap_lifecycle() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer);
        let ttl_init = env
            .storage()
            .persistent()
            .get_ttl(&DataKey::Swap(swap_id));

        client.accept_swap(&swap_id);
        let ttl_accept = env
            .storage()
            .persistent()
            .get_ttl(&DataKey::Swap(swap_id));

        client.reveal_key(&swap_id, &seller, &secret, &blinding);
        let ttl_complete = env
            .storage()
            .persistent()
            .get_ttl(&DataKey::Swap(swap_id));

        assert!(ttl_init > 0);
        assert!(ttl_accept > 0);
        assert!(ttl_complete > 0);
        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Completed);
    }
}
