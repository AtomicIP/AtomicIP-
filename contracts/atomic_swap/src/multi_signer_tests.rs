#[cfg(test)]
mod multi_signer_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _,
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

    fn setup_contract(env: &Env, registry_id: &Address) -> AtomicSwapClient {
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(registry_id);
        client
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_reveal_blocked_until_all_signers_sign() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let co_signer = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let client = setup_contract(&env, &registry_id);

        let mut signers = Vec::new(&env);
        signers.push_back(seller.clone());
        signers.push_back(co_signer.clone());

        let swap_id = client.initiate_swap_with_signers(
            &token_id, &ip_id, &seller, &1000i128, &buyer, &signers,
        );
        client.accept_swap(&swap_id);

        // Only seller has signed — reveal must fail
        client.sign_swap_reveal(&swap_id, &seller);
        let result = client.try_reveal_key(&swap_id, &seller, &secret, &blinding);
        assert!(result.is_err(), "reveal must fail when not all signers have signed");
    }

    #[test]
    fn test_reveal_succeeds_after_all_signers_sign() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let co_signer = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let client = setup_contract(&env, &registry_id);

        let mut signers = Vec::new(&env);
        signers.push_back(seller.clone());
        signers.push_back(co_signer.clone());

        let swap_id = client.initiate_swap_with_signers(
            &token_id, &ip_id, &seller, &1000i128, &buyer, &signers,
        );
        client.accept_swap(&swap_id);

        client.sign_swap_reveal(&swap_id, &seller);
        client.sign_swap_reveal(&swap_id, &co_signer);

        // All signed — reveal must succeed
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Completed);
    }

    #[test]
    fn test_non_required_signer_cannot_sign() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let co_signer = Address::generate(&env);
        let outsider = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let client = setup_contract(&env, &registry_id);

        let mut signers = Vec::new(&env);
        signers.push_back(seller.clone());
        signers.push_back(co_signer.clone());

        let swap_id = client.initiate_swap_with_signers(
            &token_id, &ip_id, &seller, &1000i128, &buyer, &signers,
        );
        client.accept_swap(&swap_id);

        let result = client.try_sign_swap_reveal(&swap_id, &outsider);
        assert!(result.is_err(), "outsider must not be able to sign");
    }

    #[test]
    fn test_duplicate_signature_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let co_signer = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let client = setup_contract(&env, &registry_id);

        let mut signers = Vec::new(&env);
        signers.push_back(seller.clone());
        signers.push_back(co_signer.clone());

        let swap_id = client.initiate_swap_with_signers(
            &token_id, &ip_id, &seller, &1000i128, &buyer, &signers,
        );
        client.accept_swap(&swap_id);

        client.sign_swap_reveal(&swap_id, &seller);
        let result = client.try_sign_swap_reveal(&swap_id, &seller);
        assert!(result.is_err(), "duplicate signature must be rejected");
    }

    #[test]
    fn test_three_signers_all_must_sign() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let signer2 = Address::generate(&env);
        let signer3 = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let client = setup_contract(&env, &registry_id);

        let mut signers = Vec::new(&env);
        signers.push_back(seller.clone());
        signers.push_back(signer2.clone());
        signers.push_back(signer3.clone());

        let swap_id = client.initiate_swap_with_signers(
            &token_id, &ip_id, &seller, &1000i128, &buyer, &signers,
        );
        client.accept_swap(&swap_id);

        // Two of three signed — still blocked
        client.sign_swap_reveal(&swap_id, &seller);
        client.sign_swap_reveal(&swap_id, &signer2);
        assert!(
            client.try_reveal_key(&swap_id, &seller, &secret, &blinding).is_err(),
            "reveal must fail with only 2 of 3 signatures"
        );

        // Third signs — now unblocked
        client.sign_swap_reveal(&swap_id, &signer3);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Completed);
    }

    #[test]
    fn test_sign_on_pending_swap_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let co_signer = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let client = setup_contract(&env, &registry_id);

        let mut signers = Vec::new(&env);
        signers.push_back(seller.clone());
        signers.push_back(co_signer.clone());

        let swap_id = client.initiate_swap_with_signers(
            &token_id, &ip_id, &seller, &1000i128, &buyer, &signers,
        );

        // Swap is still Pending — sign must fail (must be Accepted first)
        let result = client.try_sign_swap_reveal(&swap_id, &seller);
        assert!(result.is_err(), "sign must fail on a Pending swap");
    }

    #[test]
    fn test_empty_signers_list_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let client = setup_contract(&env, &registry_id);

        let empty_signers: Vec<Address> = Vec::new(&env);
        let result = client.try_initiate_swap_with_signers(
            &token_id, &ip_id, &seller, &1000i128, &buyer, &empty_signers,
        );
        assert!(result.is_err(), "empty signers list must be rejected");
    }

    // ── #527: batch_sign_swap_reveal tests ────────────────────────────────────

    /// Helper: create a swap with two required signers (seller + co_signer),
    /// accept it, and return the swap_id.
    fn setup_accepted_swap_with_signers(
        env: &Env,
        client: &AtomicSwapClient,
        token_id: &Address,
        seller: &Address,
        co_signer: &Address,
        buyer: &Address,
        ip_id: u64,
        price: i128,
    ) -> u64 {
        let mut signers = Vec::new(env);
        signers.push_back(seller.clone());
        signers.push_back(co_signer.clone());

        let swap_id = client.initiate_swap_with_signers(token_id, &ip_id, seller, &price, buyer, &signers);
        client.accept_swap(&swap_id);
        swap_id
    }

    #[test]
    fn test_batch_sign_signs_all_swaps() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let co_signer = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id1, secret1, blinding1) = setup_registry(&env, &seller);
        let (_, ip_id2, secret2, blinding2) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 10_000_000);
        let client = setup_contract(&env, &registry_id);

        let swap_id1 = setup_accepted_swap_with_signers(
            &env, &client, &token_id, &seller, &co_signer, &buyer, ip_id1, 1000,
        );
        let swap_id2 = setup_accepted_swap_with_signers(
            &env, &client, &token_id, &seller, &co_signer, &buyer, ip_id2, 2000,
        );

        // co_signer batch-signs both swaps
        let mut ids = Vec::new(&env);
        ids.push_back(swap_id1);
        ids.push_back(swap_id2);
        client.batch_sign_swap_reveal(&ids, &co_signer);

        // seller still needs to sign individually; after that reveal must succeed
        client.sign_swap_reveal(&swap_id1, &seller);
        client.sign_swap_reveal(&swap_id2, &seller);

        client.reveal_key(&swap_id1, &seller, &secret1, &blinding1);
        client.reveal_key(&swap_id2, &seller, &secret2, &blinding2);

        assert_eq!(client.get_swap(&swap_id1).unwrap().status, SwapStatus::Completed);
        assert_eq!(client.get_swap(&swap_id2).unwrap().status, SwapStatus::Completed);
    }

    #[test]
    fn test_batch_sign_outsider_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let co_signer = Address::generate(&env);
        let outsider = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let client = setup_contract(&env, &registry_id);

        let swap_id = setup_accepted_swap_with_signers(
            &env, &client, &token_id, &seller, &co_signer, &buyer, ip_id, 1000,
        );

        let mut ids = Vec::new(&env);
        ids.push_back(swap_id);
        let result = client.try_batch_sign_swap_reveal(&ids, &outsider);
        assert!(result.is_err(), "outsider must not be able to batch-sign");
    }

    #[test]
    fn test_batch_sign_duplicate_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let co_signer = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let client = setup_contract(&env, &registry_id);

        let swap_id = setup_accepted_swap_with_signers(
            &env, &client, &token_id, &seller, &co_signer, &buyer, ip_id, 1000,
        );

        let mut ids = Vec::new(&env);
        ids.push_back(swap_id);
        client.batch_sign_swap_reveal(&ids, &co_signer);

        // Second batch-sign on the same swap must fail
        let result = client.try_batch_sign_swap_reveal(&ids, &co_signer);
        assert!(result.is_err(), "duplicate batch-sign must be rejected");
    }

    #[test]
    fn test_batch_sign_pending_swap_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let co_signer = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let client = setup_contract(&env, &registry_id);

        let mut signers = Vec::new(&env);
        signers.push_back(seller.clone());
        signers.push_back(co_signer.clone());
        let swap_id = client.initiate_swap_with_signers(
            &token_id, &ip_id, &seller, &1000i128, &buyer, &signers,
        );
        // NOT accepted — still Pending

        let mut ids = Vec::new(&env);
        ids.push_back(swap_id);
        let result = client.try_batch_sign_swap_reveal(&ids, &co_signer);
        assert!(result.is_err(), "batch-sign on Pending swap must fail");
    }

    #[test]
    fn test_batch_reveal_blocked_without_all_signatures() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let co_signer = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id1, secret1, blinding1) = setup_registry(&env, &seller);
        let (_, ip_id2, secret2, blinding2) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 10_000_000);
        let client = setup_contract(&env, &registry_id);

        let swap_id1 = setup_accepted_swap_with_signers(
            &env, &client, &token_id, &seller, &co_signer, &buyer, ip_id1, 1000,
        );
        let swap_id2 = setup_accepted_swap_with_signers(
            &env, &client, &token_id, &seller, &co_signer, &buyer, ip_id2, 2000,
        );

        // Only seller signs swap_id1; nobody signs swap_id2
        client.sign_swap_reveal(&swap_id1, &seller);

        let mut ids = Vec::new(&env);
        ids.push_back(swap_id1);
        ids.push_back(swap_id2);
        let mut secrets = Vec::new(&env);
        secrets.push_back(secret1);
        secrets.push_back(secret2);
        let mut blindings = Vec::new(&env);
        blindings.push_back(blinding1);
        blindings.push_back(blinding2);

        // batch_reveal_keys must fail because not all signers have signed
        let result = client.try_batch_reveal_keys(&ids, &secrets, &blindings, &seller);
        assert!(result.is_err(), "batch_reveal_keys must fail when signers have not all signed");
    }
}
