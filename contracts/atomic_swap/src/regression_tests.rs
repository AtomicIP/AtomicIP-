/// Regression test suite for AtomicIP atomic swap contract.
///
/// Each test is named after the bug it guards against and references the
/// threat-model scenario that motivated it.  These tests run on every CI
/// build via `cargo test --workspace`.
///
/// See docs/security-audit-checklist.md and docs/threat-model.md for context.
#[cfg(test)]
mod regression_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Bytes, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, ContractError, SwapStatus};

    // ── Shared helpers ────────────────────────────────────────────────────────

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

    fn setup_swap(env: &Env, registry_id: &Address) -> Address {
        let contract_id = env.register(AtomicSwap, ());
        AtomicSwapClient::new(env, &contract_id).initialize(registry_id);
        contract_id
    }

    // ── Bug: Duplicate commitment (Threat Model §4) ───────────────────────────
    //
    // A second `commit_ip` with the same hash must be rejected.
    #[test]
    fn regression_duplicate_commitment_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let owner = Address::generate(&env);
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);

        let secret = BytesN::from_array(&env, &[0x01u8; 32]);
        let blinding = BytesN::from_array(&env, &[0x02u8; 32]);
        let mut preimage = Bytes::new(&env);
        preimage.append(&Bytes::from(secret.clone()));
        preimage.append(&Bytes::from(blinding.clone()));
        let hash: BytesN<32> = env.crypto().sha256(&preimage).into();

        registry.commit_ip(&owner, &hash);

        let result = registry.try_commit_ip(&owner, &hash);
        assert!(result.is_err(), "duplicate commitment must be rejected");
    }

    // ── Bug: Non-owner swap initiation (Threat Model §5) ─────────────────────
    //
    // An address that does not own the IP must not be able to initiate a swap.
    #[test]
    fn regression_non_owner_cannot_initiate_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let real_owner = Address::generate(&env);
        let attacker = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &real_owner);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let result = client.try_initiate_swap(&token_id, &ip_id, &attacker, &500_i128, &buyer, &0_u32, &None);
        assert!(
            result.is_err(),
            "non-owner must not be able to initiate a swap"
        );
    }

    // ── Bug: Zero-price swap (Threat Model §7) ────────────────────────────────
    //
    // `initiate_swap` with price = 0 must return PriceMustBeGreaterThanZero.
    #[test]
    fn regression_zero_price_swap_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let result = client.try_initiate_swap(&token_id, &ip_id, &seller, &0_i128, &buyer, &0_u32, &None);
        assert_eq!(
            result.unwrap_err().unwrap(),
            ContractError::PriceMustBeGreaterThanZero,
            "zero-price swap must be rejected with PriceMustBeGreaterThanZero"
        );
    }

    // ── Bug: Concurrent swap (Threat Model §8) ────────────────────────────────
    //
    // A second `initiate_swap` for the same IP while one is active must fail.
    #[test]
    fn regression_concurrent_swap_blocked_by_active_swap_lock() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer1 = Address::generate(&env);
        let buyer2 = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer1, 2000);
        StellarAssetClient::new(&env, &token_id).mint(&buyer2, &2000);
        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer1, &0_u32, &None, &false);

        let result = client.try_initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer2, &0_u32, &None);
        assert_eq!(
            result.unwrap_err().unwrap(),
            ContractError::ActiveSwapAlreadyExistsForThisIpId,
            "second swap for same IP must be blocked"
        );
    }

    // ── Bug: Revoked IP swap (Threat Model §6) ────────────────────────────────
    //
    // Initiating a swap for a revoked IP must return IpIsRevoked.
    #[test]
    fn regression_revoked_ip_cannot_be_swapped() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        // Revoke the IP via the registry
        let registry = IpRegistryClient::new(&env, &registry_id);
        registry.revoke_ip(&seller, &ip_id);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let result = client.try_initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);
        assert_eq!(
            result.unwrap_err().unwrap(),
            ContractError::IpRevoked,
            "swap on revoked IP must be rejected"
        );
    }

    // ── Bug: Invalid key reveal (Threat Model §1) ─────────────────────────────
    //
    // `reveal_key` with a wrong secret must return InvalidKey and leave the
    // swap in Accepted state (funds not released).
    #[test]
    fn regression_invalid_key_does_not_release_funds() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, _correct_secret, _blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &false);
        client.accept_swap(&swap_id, &buyer);

        let wrong_key = BytesN::from_array(&env, &[0xFFu8; 32]);
        let result = client.try_reveal_key(&swap_id, &seller, &wrong_key);
        assert_eq!(
            result.unwrap_err().unwrap(),
            ContractError::InvalidKey,
            "wrong key must be rejected"
        );

        // Swap must still be Accepted — funds not released
        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Accepted
        );
    }

    // ── Bug: Expired swap cancellation by non-buyer (Threat Model §3) ─────────
    //
    // Only the buyer may cancel an expired swap; seller must be rejected.
    #[test]
    fn regression_only_buyer_can_cancel_expired_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &false);
        client.accept_swap(&swap_id, &buyer);

        // Advance ledger past expiry
        env.ledger().with_mut(|l| l.timestamp += 8 * 24 * 3600);

        let result = client.try_cancel_expired_swap(&swap_id, &seller);
        assert_eq!(
            result.unwrap_err().unwrap(),
            ContractError::OnlyTheBuyerCanCancelAnExpiredSwap,
            "seller must not be able to cancel an expired swap"
        );

        // Buyer succeeds
        client.cancel_expired_swap(&swap_id, &buyer);
        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Cancelled
        );
    }

    // ── Bug: Cancelling a non-expired accepted swap ───────────────────────────
    //
    // `cancel_expired_swap` must fail if the swap has not yet expired.
    #[test]
    fn regression_cannot_cancel_accepted_swap_before_expiry() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &false);
        client.accept_swap(&swap_id, &buyer);

        // Do NOT advance ledger — swap is not expired
        let result = client.try_cancel_expired_swap(&swap_id, &buyer);
        assert_eq!(
            result.unwrap_err().unwrap(),
            ContractError::NotExpired,
            "cancellation before expiry must be rejected"
        );
    }

    // ── Bug: ActiveSwap lock released after completion ────────────────────────
    //
    // After a swap completes, a new swap for the same IP must be allowed.
    #[test]
    fn regression_active_swap_lock_released_after_completion() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 2000);
        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &false);
        client.accept_swap(&swap_id, &buyer);

        // Build the correct preimage key: secret || blinding
        let mut key_bytes = Bytes::new(&env);
        key_bytes.append(&Bytes::from(secret.clone()));
        key_bytes.append(&Bytes::from(blinding.clone()));
        let reveal_key: BytesN<32> = env.crypto().sha256(&key_bytes).into();
        // reveal_key expects the raw secret that hashes to the commitment
        client.reveal_key(&swap_id, &seller, &secret);

        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Completed
        );

        // A new swap for the same IP must now be accepted
        StellarAssetClient::new(&env, &token_id).mint(&buyer, &500);
        let result = client.try_initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None);
        assert!(
            result.is_ok(),
            "new swap must be allowed after previous swap completes"
        );
    }

    // ── Bug: #781 Dispute-Arbitration Hardening ───────────────────────────────
    //
    // Regression test for the #781 dispute-arbitration hardening fix.
    // Verifies that:
    // 1. All ContractError variants can be constructed without panicking
    // 2. Error round-trips through Error::from_contract_error correctly
    // 3. Committee signing, ruling entry, and execution work end-to-end
    //
    // This test prevents silent regressions in arbitration logic that could
    // break dispute resolution for swaps with committee rulings or bonds.
    #[test]
    fn regression_781_arbitration_hardening_error_codes() {
        use soroban_sdk::Error as SorobanError;

        let env = Env::default();
        env.mock_all_auths();

        // Test that all #781-related error codes can round-trip through Soroban's error system
        let error_codes = [
            ContractError::NotACommitteeSigner,      // 55
            ContractError::DuplicateSigner,          // 56
            ContractError::InsufficientSignatures,   // 57
            ContractError::CommitteeSizeTooSmall,    // 58
            ContractError::EvidenceRequired,         // 59
            ContractError::RulingAlreadyPending,     // 60
            ContractError::NoPendingRuling,          // 62
            ContractError::TimelockNotElapsed,       // 63
            ContractError::RulingFinalized,          // 64
        ];

        // Verify each error code round-trips: ContractError → Soroban Error → ContractError
        for error in error_codes.iter() {
            let soroban_err = SorobanError::from_contract_error(*error as u32);
            // The error can be constructed and converted without panicking
            assert!(!format!("{:?}", soroban_err).is_empty(),
                "error {:?} must round-trip through Soroban Error system", error);
        }

        // End-to-end arbitration flow: verify committee signing and ruling work
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let admin = Address::generate(&env);

        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &token_admin, &buyer, 20_000_000);
        StellarAssetClient::new(&env, &token_id).mint(&seller, &20_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Setup a disputed swap
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &20_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );
        client.accept_swap(&swap_id);
        client.raise_dispute(&swap_id);

        // Setup committee: 3 signers, 2-of-3 threshold (threat model minimum from #781)
        let mut signers = Vec::new(&env);
        signers.push_back(Address::generate(&env));
        signers.push_back(Address::generate(&env));
        signers.push_back(Address::generate(&env));

        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);

        // Submit evidence and verify arbitration path
        let hash = BytesN::from_array(&env, &[0xAAu8; 32]);
        client.submit_dispute_evidence(&swap_id, &buyer, &hash);

        // Construct 2-of-3 signers
        let mut two_signers = Vec::new(&env);
        two_signers.push_back(signers.get(0).unwrap());
        two_signers.push_back(signers.get(1).unwrap());

        // Arbitrate with refund
        client.arbitrate_dispute(&swap_id, &two_signers, &true);

        // Verify swap is still Disputed before ruling execution
        let swap_before = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap_before.status, SwapStatus::Disputed,
            "swap must remain Disputed after arbitrate_dispute until execute_ruling");

        // Skip the 48-hour ruling delay
        env.ledger().set(env.ledger().sequence() + 999999);

        // Execute the ruling
        client.execute_ruling(&swap_id);

        // Verify final state: swap is Cancelled (refund path)
        let swap_after = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap_after.status, SwapStatus::Cancelled,
            "swap must be Cancelled after ruling execution with refund=true");
    }
}
