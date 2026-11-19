/// #1342: initiate_swap must reject a negative collateral_amount with a structured
/// error, so no negative value is stored on the swap for later refund paths to read.
#[cfg(test)]
mod collateral_validation_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _, token::StellarAssetClient, Address, Bytes, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, ContractError};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn setup_registry(env: &Env) -> Address {
        let registry_id = env.register(IpRegistry, ());
        let _ = IpRegistryClient::new(env, &registry_id);
        registry_id
    }

    fn commit_ip(env: &Env, registry_id: &Address, owner: &Address, seed: u8) -> u64 {
        let registry = IpRegistryClient::new(env, registry_id);
        let secret = BytesN::from_array(env, &[seed; 32]);
        let blinding = BytesN::from_array(env, &[seed.wrapping_add(0x80); 32]);
        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(secret));
        preimage.append(&Bytes::from(blinding));
        let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        registry.commit_ip(owner, &hash, &0u32)
    }

    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    fn setup_contract(env: &Env, registry_id: &Address) -> Address {
        let contract_id = env.register(AtomicSwap, ());
        AtomicSwapClient::new(env, &contract_id).initialize(registry_id);
        contract_id
    }

    // ── #1342: negative collateral is rejected ───────────────────────────────

    #[test]
    fn test_initiate_swap_rejects_negative_collateral() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);
        let ip_id = commit_ip(&env, &registry_id, &seller, 0x21);

        let result = client.try_initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer, &0u32, &None, &-1i128, &false,
        );

        match result {
            Err(Ok(err)) => assert_eq!(
                err,
                soroban_sdk::Error::from_contract_error(ContractError::InvalidCollateral as u32),
                "negative collateral must fail with InvalidCollateral"
            ),
            other => panic!("negative collateral must be rejected, got {other:?}"),
        }
    }

    #[test]
    fn test_initiate_swap_accepts_zero_collateral() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = setup_registry(&env);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = setup_contract(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);
        let ip_id = commit_ip(&env, &registry_id, &seller, 0x22);

        // Boundary: zero collateral means "no collateral" and must still be allowed.
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer, &0u32, &None, &0i128, &false,
        );
        assert_eq!(client.get_swap(&swap_id).unwrap().collateral_amount, 0);
    }
}
