#[cfg(test)]
mod required_approvals_bound_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _, token::StellarAssetClient, Address, Bytes, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, MAX_REQUIRED_APPROVALS};

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
    fn test_zero_required_approvals_is_accepted_at_creation() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, PRICE);
        let client = setup_contract(&env, &registry_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &PRICE, &buyer, &0u32, &None, &0i128, &false,
        );

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.required_approvals, 0);
    }

    #[test]
    fn test_satisfiable_required_approvals_can_be_accepted() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let approver_a = Address::generate(&env);
        let approver_b = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, PRICE);
        let client = setup_contract(&env, &registry_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &PRICE, &buyer, &2u32, &None, &0i128, &false,
        );

        client.approve_swap(&swap_id, &approver_a);
        client.approve_swap(&swap_id, &approver_b);
        client.accept_swap(&swap_id);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, crate::SwapStatus::Accepted);
    }

    #[test]
    fn test_unsatisfiable_required_approvals_rejected_at_creation() {
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
            &(MAX_REQUIRED_APPROVALS + 1),
            &None,
            &0i128,
            &false,
        );
        assert!(result.is_err());

        // The IP must not be left locked by a rejected swap.
        assert!(client.get_swaps_by_ip(&ip_id).is_none());
    }

    #[test]
    fn test_max_required_approvals_is_accepted_at_creation() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, PRICE);
        let client = setup_contract(&env, &registry_id);

        let swap_id = client.initiate_swap(
            &token_id,
            &ip_id,
            &seller,
            &PRICE,
            &buyer,
            &MAX_REQUIRED_APPROVALS,
            &None,
            &0i128,
            &false,
        );

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.required_approvals, MAX_REQUIRED_APPROVALS);
    }
}
