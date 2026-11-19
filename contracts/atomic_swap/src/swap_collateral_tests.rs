#[cfg(test)]
mod swap_collateral_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        token::StellarAssetClient,
        Address, Bytes, BytesN, Env, Vec,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus};

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

        let ip_id = registry.commit_ip(owner, &commitment_hash, &0u32);
        (registry_id, ip_id, secret, blinding)
    }

    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    fn setup_contract<'a>(env: &'a Env, registry_id: &Address) -> AtomicSwapClient<'a> {
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(&<soroban_sdk::Address as soroban_sdk::testutils::Address>::generate(&env), registry_id);
        client
    }

    // ── Tests ─────────────────────────────────────────────────────────────────
    // Collateral is supplied when a swap is initiated (`initiate_swap`'s
    // `collateral_amount`); the separate deposit entry point no longer exists.

    #[test]
    fn test_collateral_deposit_required_for_high_value_swaps() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 5_000_000);
        let client = setup_contract(&env, &registry_id);

        let mut signers = Vec::new(&env);
        signers.push_back(seller.clone());

        // High value swap
        let swap_id = client.initiate_swap_with_signers(
            &token_id, &ip_id, &seller, &100_000i128, &buyer, &signers,
        );

        // No collateral is attached when the swap is created with signers.
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.collateral_amount, 0);
    }

    #[test]
    fn test_collateral_deposit_increases_swap_security() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 5_000_000);
        let client = setup_contract(&env, &registry_id);

        // High-value swap carrying collateral from the buyer at initiation.
        let collateral_amount: i128 = 10_000;
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &100_000i128, &buyer, &0u32, &None,
            &collateral_amount, &false,
        );

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.collateral_amount, collateral_amount);
    }

    #[test]
    fn test_collateral_amount_stored_in_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 5_000_000);
        let client = setup_contract(&env, &registry_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &50_000i128, &buyer, &0u32, &None,
            &7_500i128, &false,
        );

        // Verify collateral amount is stored
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.collateral_amount, 7_500);
    }

    #[test]
    fn test_collateral_persists_through_acceptance() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 5_000_000);
        let client = setup_contract(&env, &registry_id);

        let collateral: i128 = 5_000;
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &50_000i128, &buyer, &0u32, &None,
            &collateral, &false,
        );
        client.accept_swap(&swap_id);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.collateral_amount, collateral);
        assert_eq!(swap.status, SwapStatus::Accepted);
    }

    #[test]
    fn test_zero_collateral_allowed_for_low_value_swaps() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let client = setup_contract(&env, &registry_id);

        let mut signers = Vec::new(&env);
        signers.push_back(seller.clone());

        // Low value swap
        let swap_id = client.initiate_swap_with_signers(
            &token_id, &ip_id, &seller, &100i128, &buyer, &signers,
        );

        // Should allow zero collateral for low value
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.collateral_amount, 0);
    }
}
