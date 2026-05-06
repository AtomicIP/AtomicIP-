#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{storage::Persistent, Address as _, Ledger, Events},
        token::StellarAssetClient,
        Address, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, DataKey, ProtocolConfig, SwapStatus};

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

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &false);

        let ttl = env.storage().persistent().get_ttl(&DataKey::Swap(swap_id));
        assert!(ttl > 0, "TTL should be set after swap initiation");
        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Pending
        );
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

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &false);
        client.accept_swap(&swap_id);

        let ttl = env.storage().persistent().get_ttl(&DataKey::Swap(swap_id));
        assert!(ttl > 0, "TTL should be extended after swap acceptance");
        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Accepted
        );
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

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &false);
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        let ttl = env.storage().persistent().get_ttl(&DataKey::Swap(swap_id));
        assert!(ttl > 0, "TTL should be extended after swap completion");
        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Completed
        );
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

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &false);
        client.cancel_swap(&swap_id, &seller);

        let ttl = env.storage().persistent().get_ttl(&DataKey::Swap(swap_id));
        assert!(ttl > 0, "TTL should be extended after swap cancellation");
        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Cancelled
        );
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

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &false);
        let ttl_init = env.storage().persistent().get_ttl(&DataKey::Swap(swap_id));

        client.accept_swap(&swap_id);
        let ttl_accept = env.storage().persistent().get_ttl(&DataKey::Swap(swap_id));

        client.reveal_key(&swap_id, &seller, &secret, &blinding);
        let ttl_complete = env.storage().persistent().get_ttl(&DataKey::Swap(swap_id));

        assert!(ttl_init > 0);
        assert!(ttl_accept > 0);
        assert!(ttl_complete > 0);
        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Completed
        );
    }

    #[test]
    fn test_protocol_config_is_cached_in_instance_storage() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let contract_id = setup_swap(&env, &setup_registry(&env, &seller).0);
        let client = AtomicSwapClient::new(&env, &contract_id);
        let treasury = Address::generate(&env);

        client.admin_set_protocol_config(&250u32, &treasury, &3_600u64, &2_592_000u64, &0_u32);

        let cached = env
            .storage()
            .instance()
            .get::<DataKey, ProtocolConfig>(&DataKey::ProtocolConfig)
            .unwrap();

        assert_eq!(cached.protocol_fee_bps, 250);
        assert_eq!(cached.treasury, treasury);
        assert_eq!(cached.dispute_window_seconds, 3_600);
    }

    #[test]
    fn test_protocol_config_update_invalidates_cache() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let contract_id = setup_swap(&env, &setup_registry(&env, &seller).0);
        let client = AtomicSwapClient::new(&env, &contract_id);
        let treasury_a = Address::generate(&env);
        let treasury_b = Address::generate(&env);

        client.admin_set_protocol_config(&100u32, &treasury_a, &900u64, &2_592_000u64, &0_u32);
        client.admin_set_protocol_config(&500u32, &treasury_b, &7_200u64, &2_592_000u64, &0_u32);

        let cached = env
            .storage()
            .instance()
            .get::<DataKey, ProtocolConfig>(&DataKey::ProtocolConfig)
            .unwrap();
        let persisted = env
            .storage()
            .persistent()
            .get::<DataKey, ProtocolConfig>(&DataKey::ProtocolConfig)
            .unwrap();

        assert_eq!(cached.protocol_fee_bps, 500);
        assert_eq!(cached.treasury, treasury_b);
        assert_eq!(cached.dispute_window_seconds, 7_200);
        assert_eq!(persisted, cached);
        assert_eq!(client.get_protocol_config(), cached);
    }

    #[test]
    fn test_key_revealed_event_fee_breakdown() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let price = 1000_i128;
        let token_id = setup_token(&env, &admin, &buyer, price);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        // 250 bps = 2.5% fee
        client.admin_set_protocol_config(&250u32, &treasury, &86400u64, &2_592_000u64, &0_u32);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &price, &buyer, &0_u32, &None, &false);
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        let expected_fee = (price * 250) / 10000; // 25
        let expected_seller = price - expected_fee;  // 975

        let events = env.events().all();
        let key_rev_event = events.iter().find(|(_, topics, _)| {
            topics == &soroban_sdk::vec![&env, soroban_sdk::symbol_short!("key_rev").into()]
        });
        assert!(key_rev_event.is_some(), "key_rev event not found");

        let (_, _, data) = key_rev_event.unwrap();
        let event: crate::KeyRevealedEvent = soroban_sdk::FromVal::from_val(&env, &data);
        assert_eq!(event.swap_id, swap_id);
        assert_eq!(event.fee_amount, expected_fee);
        assert_eq!(event.seller_amount, expected_seller);
    }

    #[test]
    fn test_auto_resolve_dispute_refunds_buyer_after_timeout() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let price = 1000_i128;
        let token_id = setup_token(&env, &admin, &buyer, price);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        // 1-second dispute window, 100-second resolution timeout
        client.admin_set_protocol_config(&0u32, &treasury, &1u64, &100u64, &0_u32);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &price, &buyer, &0_u32, &None, &false);
        client.accept_swap(&swap_id);

        // Advance time past dispute window so buyer can raise dispute
        env.ledger().with_mut(|l| l.timestamp = 10);
        // Wait — dispute window is 1s, accept_timestamp is 0, elapsed = 10 >= 1 → expired
        // So we need to raise dispute BEFORE the window expires. Reset to within window.
        env.ledger().with_mut(|l| l.timestamp = 0);
        client.raise_dispute(&swap_id);

        // Advance past resolution timeout (100s from dispute_timestamp=0)
        env.ledger().with_mut(|l| l.timestamp = 101);
        client.auto_resolve_dispute(&swap_id);

        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Cancelled);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #25)")]
    fn test_auto_resolve_dispute_rejected_before_timeout() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let price = 500_i128;
        let token_id = setup_token(&env, &admin, &buyer, price);

        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        // 1-second dispute window, 1000-second resolution timeout
        client.admin_set_protocol_config(&0u32, &treasury, &1u64, &1000u64, &0_u32);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &price, &buyer, &0_u32, &None, &false);
        client.accept_swap(&swap_id);
        client.raise_dispute(&swap_id);

        // Only 50s elapsed — timeout not reached
        env.ledger().with_mut(|l| l.timestamp = 50);
        client.auto_resolve_dispute(&swap_id); // must panic DisputeResolutionTimeout=25
    }

    #[test]
    fn test_cancel_swap_stores_manual_cancel_reason() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);
        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &false);
        client.cancel_swap(&swap_id, &seller);

        let reason = client.get_cancellation_reason(&swap_id).unwrap();
        assert_eq!(reason, soroban_sdk::Bytes::from_slice(&env, b"manual_cancel"));
    }

    #[test]
    fn test_cancel_expired_swap_stores_expired_reason() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);
        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &false);
        client.accept_swap(&swap_id);
        // Advance past expiry (initiation timestamp=0, expiry=604800)
        env.ledger().with_mut(|l| l.timestamp = 604801);
        client.cancel_expired_swap(&swap_id, &buyer);

        let reason = client.get_cancellation_reason(&swap_id).unwrap();
        assert_eq!(reason, soroban_sdk::Bytes::from_slice(&env, b"expired"));
    }

    #[test]
    fn test_no_reason_for_non_cancelled_swap() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);
        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &false);
        assert!(client.get_cancellation_reason(&swap_id).is_none());
    }

    // ── #360: Admin Rollback Tests ───────────────────────────────────────────

    #[test]
    fn test_admin_rollback_pending_swap() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);
        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &false);
        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Pending);

        let reason = soroban_sdk::Bytes::from_slice(&env, b"fraudulent_seller");
        client.admin_rollback_swap(&swap_id, &admin, &reason);

        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Cancelled);
        let stored_reason = client.get_cancellation_reason(&swap_id).unwrap();
        assert_eq!(stored_reason, reason);
    }

    #[test]
    fn test_admin_rollback_accepted_swap_refunds_buyer() {
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
        client.accept_swap(&swap_id);
        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Accepted);

        let reason = soroban_sdk::Bytes::from_slice(&env, b"buyer_complaint");
        client.admin_rollback_swap(&swap_id, &admin, &reason);

        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Cancelled);
        // Buyer should be refunded (price was 500)
        let buyer_balance = StellarAssetClient::new(&env, &token_id).balance(&buyer);
        assert_eq!(buyer_balance, 1000); // Original 1000 - 500 (paid) + 500 (refund) = 1000
    }

    #[test]
    fn test_admin_rollback_disputed_swap_refunds_buyer() {
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
        client.accept_swap(&swap_id);
        client.raise_dispute(&swap_id);
        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Disputed);

        let reason = soroban_sdk::Bytes::from_slice(&env, b"mutual_agreement");
        client.admin_rollback_swap(&swap_id, &admin, &reason);

        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Cancelled);
        let buyer_balance = StellarAssetClient::new(&env, &token_id).balance(&buyer);
        assert_eq!(buyer_balance, 1000);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #23)")]
    fn test_admin_rollback_rejected_for_non_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);
        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &false);
        // Try to rollback with non-admin (seller)
        let reason = soroban_sdk::Bytes::from_slice(&env, b"unauthorized");
        client.admin_rollback_swap(&swap_id, &seller, &reason);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #11)")]
    fn test_admin_rollback_rejected_for_completed_swap() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, secret, blinding) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);
        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &false);
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);
        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Completed);

        // Try to rollback a completed swap - should fail
        let reason = soroban_sdk::Bytes::from_slice(&env, b"too_late");
        client.admin_rollback_swap(&swap_id, &admin, &reason);
    }

    #[test]
    fn test_admin_rollback_with_collateral_refunds_buyer() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1500);
        let contract_id = setup_swap(&env, &registry_id);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let swap_id = client.initiate_swap_with_collateral(
            &token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None,
            &100_i128, // collateral
        );
        client.accept_swap(&swap_id);
        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Accepted);

        let reason = soroban_sdk::Bytes::from_slice(&env, b"collateral_refund");
        client.admin_rollback_swap(&swap_id, &admin, &reason);

        assert_eq!(client.get_swap(&swap_id).unwrap().status, SwapStatus::Cancelled);
        // Buyer should be refunded: 1500 - 500 (price) - 100 (collateral) + 500 + 100 = 1500
        let buyer_balance = StellarAssetClient::new(&env, &token_id).balance(&buyer);
        assert_eq!(buyer_balance, 1500);
    }
}
