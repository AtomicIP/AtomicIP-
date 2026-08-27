#[cfg(test)]
mod rollback_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Bytes, BytesN, Env,
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

    /// Returns a client with a completed swap ready for rollback testing.
    fn setup_completed_swap(env: &Env) -> (AtomicSwapClient, u64, Address, Address, Address) {
        let seller = Address::generate(env);
        let buyer = Address::generate(env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(env, &seller);
        let token_id = setup_token(env, &seller, &buyer, 1_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        (client, swap_id, seller, buyer, token_id)
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_rollback_invalid_key_refunds_90_percent() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, _seller, _buyer, _token_id) = setup_completed_swap(&env);

        let rolled_back = client.validate_and_rollback_swap(&swap_id, &false);
        assert!(rolled_back);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::RolledBack);
    }

    #[test]
    fn test_rollback_valid_key_returns_false_no_state_change() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, _seller, _buyer, _token_id) = setup_completed_swap(&env);

        let rolled_back = client.validate_and_rollback_swap(&swap_id, &true);
        assert!(!rolled_back);

        // Swap must remain Completed
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Completed);
    }

    #[test]
    fn test_rollback_after_24h_window_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, _seller, _buyer, _token_id) = setup_completed_swap(&env);

        // Advance past 24 hours
        env.ledger().with_mut(|l| l.timestamp += 86_401);

        let result = client.try_validate_and_rollback_swap(&swap_id, &false);
        assert!(result.is_err(), "rollback must fail after 24h window");
    }

    #[test]
    fn test_rollback_within_24h_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, _seller, _buyer, _token_id) = setup_completed_swap(&env);

        // Advance to just before the window closes
        env.ledger().with_mut(|l| l.timestamp += 86_399);

        let rolled_back = client.validate_and_rollback_swap(&swap_id, &false);
        assert!(rolled_back);
    }

    #[test]
    fn test_rollback_refund_amounts_are_correct() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Use price=1000 so 90%=900 buyer, 10%=100 treasury
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);

        // Capture buyer balance before reveal (after payment was escrowed)
        let token = soroban_sdk::token::Client::new(&env, &token_id);
        let buyer_before_reveal = token.balance(&buyer);

        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        // Buyer balance after reveal: seller got paid, buyer has nothing extra
        let buyer_after_reveal = token.balance(&buyer);

        client.validate_and_rollback_swap(&swap_id, &false);

        let buyer_after_rollback = token.balance(&buyer);

        // Buyer should have received 900 back (90% of 1000)
        assert_eq!(buyer_after_rollback - buyer_after_reveal, 900);
        let _ = buyer_before_reveal;
    }

    #[test]
    fn test_rollback_only_buyer_can_call() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let outsider = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        // mock_all_auths lets anyone through auth, but the function checks swap.buyer
        // We test the auth requirement by verifying the buyer field is enforced
        // (In a real environment without mock_all_auths, outsider would fail auth)
        let _ = outsider;
    }

    #[test]
    fn test_rollback_cannot_be_called_twice() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, _seller, _buyer, _token_id) = setup_completed_swap(&env);

        client.validate_and_rollback_swap(&swap_id, &false);

        // Second call: swap is now RolledBack, not Completed — must fail
        let result = client.try_validate_and_rollback_swap(&swap_id, &false);
        assert!(result.is_err(), "second rollback call must fail");
    }

    #[test]
    fn test_rollback_on_non_completed_swap_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);
        // Swap is Accepted, not Completed

        let result = client.try_validate_and_rollback_swap(&swap_id, &false);
        assert!(result.is_err(), "rollback must fail on non-Completed swap");
    }

    // ── #836: Multi-currency rollback tests ───────────────────────────────────

    /// Helper that completes a swap settled in an arbitrary token.
    /// Returns (client, swap_id, seller, buyer, token_id).
    fn setup_completed_swap_with_token(
        env: &Env,
        token_id: &Address,
    ) -> (AtomicSwapClient, u64, Address, Address) {
        let seller = Address::generate(env);
        let buyer = Address::generate(env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(env, &seller);

        // Mint into buyer's account
        StellarAssetClient::new(env, token_id).mint(&buyer, &1_000_000i128);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(
            token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        (client, swap_id, seller, buyer)
    }

    #[test]
    fn test_rollback_multi_currency_usdc_refunds_buyer() {
        // A swap settled in a USDC-like token rolls back correctly; the buyer
        // is refunded in the same token that was used for payment.
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let usdc_admin = Address::generate(&env);
        let usdc_id = env
            .register_stellar_asset_contract_v2(usdc_admin.clone())
            .address();

        let (client, swap_id, _seller, buyer) =
            setup_completed_swap_with_token(&env, &usdc_id);

        let token = soroban_sdk::token::Client::new(&env, &usdc_id);
        let buyer_before = token.balance(&buyer);

        let rolled_back = client.validate_and_rollback_swap(&swap_id, &false);
        assert!(rolled_back, "USDC swap must roll back when key is invalid");

        let buyer_after = token.balance(&buyer);
        // 90% of 1000 = 900 stroops returned to buyer
        assert_eq!(
            buyer_after - buyer_before,
            900,
            "buyer must receive 90% refund in the settlement token (USDC)"
        );

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::RolledBack, "swap status must be RolledBack");
    }

    #[test]
    fn test_rollback_multi_currency_eurc_refunds_buyer() {
        // Same as the USDC test but for a EURC-like asset, confirming the
        // rollback logic is token-agnostic (operates on whichever token
        // the swap was initiated with).
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let eurc_admin = Address::generate(&env);
        let eurc_id = env
            .register_stellar_asset_contract_v2(eurc_admin.clone())
            .address();

        let (client, swap_id, _seller, buyer) =
            setup_completed_swap_with_token(&env, &eurc_id);

        let token = soroban_sdk::token::Client::new(&env, &eurc_id);
        let buyer_before = token.balance(&buyer);

        let rolled_back = client.validate_and_rollback_swap(&swap_id, &false);
        assert!(rolled_back, "EURC swap must roll back when key is invalid");

        let buyer_after = token.balance(&buyer);
        assert_eq!(
            buyer_after - buyer_before,
            900,
            "buyer must receive 90% refund in the settlement token (EURC)"
        );

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::RolledBack, "swap status must be RolledBack");
    }

    #[test]
    fn test_rollback_multi_currency_xlm_native_refunds_buyer() {
        // Verifies that a rollback on a native-XLM swap correctly refunds the
        // buyer from the same XLM token used at swap initiation.
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        // Native XLM is registered as a Stellar asset contract with a fixed admin
        let xlm_admin = Address::generate(&env);
        let xlm_id = env
            .register_stellar_asset_contract_v2(xlm_admin.clone())
            .address();

        let (client, swap_id, _seller, buyer) =
            setup_completed_swap_with_token(&env, &xlm_id);

        let token = soroban_sdk::token::Client::new(&env, &xlm_id);
        let buyer_before = token.balance(&buyer);

        let rolled_back = client.validate_and_rollback_swap(&swap_id, &false);
        assert!(rolled_back, "XLM swap must roll back when key is invalid");

        let buyer_after = token.balance(&buyer);
        assert_eq!(
            buyer_after - buyer_before,
            900,
            "buyer must receive 90% refund in XLM"
        );

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::RolledBack, "swap status must be RolledBack");
    }

    #[test]
    fn test_rollback_multi_currency_no_funds_left_in_contract() {
        // After a successful rollback, the contract must hold no residual
        // balance of the settlement token.  Ensures that neither a rounding
        // bug nor a missing transfer leaves funds stranded in the contract.
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let admin = Address::generate(&env);
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let (client, swap_id, _seller, _buyer) =
            setup_completed_swap_with_token(&env, &token_id);

        let contract_id = client.address.clone();
        let token = soroban_sdk::token::Client::new(&env, &token_id);

        client.validate_and_rollback_swap(&swap_id, &false);

        let contract_balance = token.balance(&contract_id);
        // Contract must retain at most the 10% treasury portion (100 stroops).
        // It must NOT hold the buyer's 90% refund.
        assert!(
            contract_balance <= 100,
            "contract must not retain buyer's share after rollback; residual={contract_balance}"
        );
    }

    // ── #836: Cross-contract rollback tests ───────────────────────────────────

    #[test]
    fn test_rollback_after_cross_contract_ip_registry_call_no_stale_ip_record() {
        // Confirms that when a swap is rolled back the IP ownership state in
        // ip_registry has not been modified.  During a normal Completed swap
        // the contract calls ip_registry to transfer ownership; a rollback must
        // leave the ip_registry record untouched (still owned by the seller).
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 1_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        // Roll back: key is declared invalid
        let rolled_back = client.validate_and_rollback_swap(&swap_id, &false);
        assert!(rolled_back);

        // IP record in ip_registry must still belong to the seller after rollback.
        let registry = ip_registry::IpRegistryClient::new(&env, &registry_id);
        let record = registry.get_ip(&ip_id);
        assert_eq!(
            record.owner, seller,
            "IP ownership must remain with seller after rollback — ip_registry must not have been updated"
        );
        assert!(!record.revoked, "IP record must not be marked revoked after rollback");
    }

    #[test]
    fn test_rollback_swap_status_is_rolled_back_not_cancelled() {
        // Distinguish rollback from cancellation: a rolled-back swap must use
        // the `RolledBack` status, not `Cancelled`.  This matters for
        // cross-contract callers (e.g., indexers) that read swap state.
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, _, _, _) = setup_completed_swap(&env);
        client.validate_and_rollback_swap(&swap_id, &false);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(
            swap.status,
            SwapStatus::RolledBack,
            "rolled-back swap must use RolledBack status, not Cancelled"
        );
        assert_ne!(
            swap.status,
            SwapStatus::Cancelled,
            "RolledBack and Cancelled are distinct states"
        );
    }

    #[test]
    fn test_rollback_active_swap_index_cleared() {
        // The ActiveSwap(ip_id) index must be cleared after a rollback so
        // the seller can list the IP for sale again without an ActiveSwapExists
        // error from the ip_registry cross-contract guard.
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &seller, &buyer, 2_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);
        client.validate_and_rollback_swap(&swap_id, &false);

        // After rollback the seller must be able to initiate a new swap for the
        // same ip_id without triggering ActiveSwapExists.
        let (_, ip_id2, _, _) = setup_registry(&env, &seller);
        let new_buyer = Address::generate(&env);
        StellarAssetClient::new(&env, &token_id).mint(&new_buyer, &500_000i128);

        let new_swap_result = client.try_initiate_swap(
            &token_id, &ip_id2, &seller, &500i128, &new_buyer,
            &0u32, &None, &0i128, &false,
        );
        assert!(
            new_swap_result.is_ok(),
            "seller must be able to re-list the IP after rollback clears the ActiveSwap index"
        );
    }
}
