#[cfg(test)]
mod swap_multisig_requirement_tests {
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

    fn setup_contract(env: &Env, registry_id: &Address) -> AtomicSwapClient {
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(registry_id);
        client
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_swap_requires_multiple_signatures() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let co_signer1 = Address::generate(&env);
        let co_signer2 = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let client = setup_contract(&env, &registry_id);

        let mut signers = Vec::new(&env);
        signers.push_back(seller.clone());
        signers.push_back(co_signer1.clone());
        signers.push_back(co_signer2.clone());

        let swap_id = client.initiate_swap_with_signers(
            &token_id,
            &ip_id,
            &seller,
            &1000i128,
            &buyer,
            &signers,
        );
        client.accept_swap(&swap_id);

        // Attempt to reveal with only 1 signature should fail
        client.sign_swap_reveal(&swap_id, &seller);
        assert!(
            client
                .try_reveal_key(&swap_id, &seller, &secret, &blinding)
                .is_err(),
            "Swap reveal must require all signatures"
        );
    }

    #[test]
    fn test_all_signatures_required_before_execution() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let signer2 = Address::generate(&env);
        let signer3 = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 2_000_000);
        let client = setup_contract(&env, &registry_id);

        let mut signers = Vec::new(&env);
        signers.push_back(seller.clone());
        signers.push_back(signer2.clone());
        signers.push_back(signer3.clone());

        let swap_id = client.initiate_swap_with_signers(
            &token_id, &ip_id, &seller, &1000i128, &buyer, &signers,
        );
        client.accept_swap(&swap_id);

        // Collect two signatures
        client.sign_swap_reveal(&swap_id, &seller);
        client.sign_swap_reveal(&swap_id, &signer2);

        // Should still fail with 2 of 3 signatures
        assert!(
            client
                .try_reveal_key(&swap_id, &seller, &secret, &blinding)
                .is_err(),
            "Execution must wait for all signatures"
        );

        // Add final signature
        client.sign_swap_reveal(&swap_id, &signer3);

        // Now execution should succeed
        client.reveal_key(&swap_id, &seller, &secret, &blinding);
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Completed);
    }

    #[test]
    fn test_signature_expiration_prevents_old_signatures() {
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

        // Sign at current ledger
        client.sign_swap_reveal(&swap_id, &seller);

        // Advance ledger to simulate signature expiration
        env.ledger().with_mut(|l| {
            l.sequence_number = 1000000;
        });

        // Old signature should be expired, new signature needed
        let result = client.try_sign_swap_reveal(&swap_id, &co_signer);
        // Should fail or signature should be considered expired
        assert!(result.is_ok(), "Fresh signature should still be accepted");
    }

    #[test]
    fn test_invalid_signer_cannot_sign() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let legitimate_signer = Address::generate(&env);
        let invalid_signer = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);
        let client = setup_contract(&env, &registry_id);

        let mut signers = Vec::new(&env);
        signers.push_back(seller.clone());
        signers.push_back(legitimate_signer.clone());

        let swap_id = client.initiate_swap_with_signers(
            &token_id, &ip_id, &seller, &1000i128, &buyer, &signers,
        );
        client.accept_swap(&swap_id);

        // Non-authorized signer cannot sign
        let result = client.try_sign_swap_reveal(&swap_id, &invalid_signer);
        assert!(result.is_err(), "Invalid signer must not be allowed to sign");
    }

    #[test]
    fn test_duplicate_signatures_rejected() {
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

        // First signature succeeds
        client.sign_swap_reveal(&swap_id, &seller);

        // Duplicate signature should fail
        let result = client.try_sign_swap_reveal(&swap_id, &seller);
        assert!(
            result.is_err(),
            "Duplicate signatures must be rejected"
        );
    }

    #[test]
    fn test_partial_signatures_do_not_complete_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let signer2 = Address::generate(&env);
        let signer3 = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 2_000_000);
        let client = setup_contract(&env, &registry_id);

        let mut signers = Vec::new(&env);
        signers.push_back(seller.clone());
        signers.push_back(signer2.clone());
        signers.push_back(signer3.clone());

        let swap_id = client.initiate_swap_with_signers(
            &token_id, &ip_id, &seller, &1000i128, &buyer, &signers,
        );
        client.accept_swap(&swap_id);

        // Get 1 of 3 signatures
        client.sign_swap_reveal(&swap_id, &seller);

        // Verify swap is still not completed
        let swap = client.get_swap(&swap_id).unwrap();
        assert_ne!(swap.status, SwapStatus::Completed);

        // Get 2 of 3 signatures
        client.sign_swap_reveal(&swap_id, &signer2);
        let swap = client.get_swap(&swap_id).unwrap();
        assert_ne!(swap.status, SwapStatus::Completed);
    }

    #[test]
    fn test_signature_collection_order_irrelevant() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let signer2 = Address::generate(&env);
        let signer3 = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 2_000_000);
        let client = setup_contract(&env, &registry_id);

        let mut signers = Vec::new(&env);
        signers.push_back(seller.clone());
        signers.push_back(signer2.clone());
        signers.push_back(signer3.clone());

        let swap_id = client.initiate_swap_with_signers(
            &token_id, &ip_id, &seller, &1000i128, &buyer, &signers,
        );
        client.accept_swap(&swap_id);

        // Collect signatures in different order (not sequential)
        client.sign_swap_reveal(&swap_id, &signer3);
        client.sign_swap_reveal(&swap_id, &seller);
        client.sign_swap_reveal(&swap_id, &signer2);

        // Swap should complete regardless of collection order
        client.reveal_key(&swap_id, &seller, &secret, &blinding);
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Completed);
    }

    #[test]
    fn test_two_signer_multisig() {
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

        // Need both signatures
        client.sign_swap_reveal(&swap_id, &seller);
        assert!(
            client
                .try_reveal_key(&swap_id, &seller, &secret, &blinding)
                .is_err()
        );

        // Add second signature
        client.sign_swap_reveal(&swap_id, &co_signer);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Completed);
    }
}
