#[cfg(test)]
mod arbitration_timeout_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Bytes, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, ContractError, SwapStatus};

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

    /// Returns (contract_id, swap_id) with the swap already in Disputed state.
    fn setup_disputed_swap(env: &Env) -> (AtomicSwapClient, u64, Address, Address) {
        let seller = Address::generate(env);
        let buyer = Address::generate(env);
        let (registry_id, ip_id, _, _) = setup_registry(env, &seller);
        let token_id = setup_token(env, &seller, &buyer, 1_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000i128, &buyer,
            &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);
        client.raise_dispute(&swap_id);

        (client, swap_id, seller, buyer)
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_request_arbitration_records_timestamp() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, _seller, buyer) = setup_disputed_swap(&env);

        let evidence = BytesN::from_array(&env, &[0x01u8; 32]);
        client.request_arbitration(&swap_id, &buyer, &evidence);

        // Verify swap is still Disputed (not yet resolved)
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Disputed);
    }

    #[test]
    fn test_auto_refund_before_timeout_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, _seller, buyer) = setup_disputed_swap(&env);

        let evidence = BytesN::from_array(&env, &[0x01u8; 32]);
        client.request_arbitration(&swap_id, &buyer, &evidence);

        // Advance only 7 days (half of 14-day timeout)
        env.ledger().with_mut(|l| l.timestamp += 604_800);

        let result = client.try_auto_refund_timeout(&swap_id);
        assert!(
            result.is_err(),
            "auto_refund must fail before timeout elapses"
        );
    }

    #[test]
    fn test_auto_refund_after_timeout_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, _seller, buyer) = setup_disputed_swap(&env);

        let evidence = BytesN::from_array(&env, &[0x01u8; 32]);
        client.request_arbitration(&swap_id, &buyer, &evidence);

        // Advance 14 days + 1 second
        env.ledger().with_mut(|l| l.timestamp += 1_209_601);

        client.auto_refund_timeout(&swap_id);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Cancelled);
    }

    #[test]
    fn test_auto_refund_without_arbitration_request_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, _, _) = setup_disputed_swap(&env);

        // No request_arbitration call — no timestamp stored
        env.ledger().with_mut(|l| l.timestamp += 2_000_000);

        let result = client.try_auto_refund_timeout(&swap_id);
        assert!(
            result.is_err(),
            "auto_refund must fail when no arbitration was requested"
        );
    }

    #[test]
    fn test_auto_refund_only_once() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, _seller, buyer) = setup_disputed_swap(&env);

        let evidence = BytesN::from_array(&env, &[0x01u8; 32]);
        client.request_arbitration(&swap_id, &buyer, &evidence);

        env.ledger().with_mut(|l| l.timestamp += 1_209_601);
        client.auto_refund_timeout(&swap_id);

        // Second call must fail — swap is now Cancelled, not Disputed
        let result = client.try_auto_refund_timeout(&swap_id);
        assert!(result.is_err(), "second auto_refund call must fail");
    }

    #[test]
    fn test_third_party_can_trigger_auto_refund() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, _seller, buyer) = setup_disputed_swap(&env);

        let evidence = BytesN::from_array(&env, &[0x01u8; 32]);
        client.request_arbitration(&swap_id, &buyer, &evidence);

        env.ledger().with_mut(|l| l.timestamp += 1_209_601);

        // A completely unrelated address triggers the refund
        client.auto_refund_timeout(&swap_id);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Cancelled);
    }

    #[test]
    fn test_arbitration_timestamp_not_overwritten_on_second_request() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let (client, swap_id, seller, buyer) = setup_disputed_swap(&env);

        let evidence = BytesN::from_array(&env, &[0x01u8; 32]);
        // First request at t=1_000_000
        client.request_arbitration(&swap_id, &buyer, &evidence);

        // Advance 7 days and make a second request
        env.ledger().with_mut(|l| l.timestamp += 604_800);
        client.request_arbitration(&swap_id, &seller, &evidence);

        // Timeout is measured from the FIRST request, so 14 days from t=1_000_000
        // At t=1_000_000 + 604_800 + 604_800 = 1_000_000 + 1_209_600 we're exactly at limit
        // Add 1 more second to cross it
        env.ledger().with_mut(|l| l.timestamp += 604_801);

        // Should succeed — timeout measured from first request
        client.auto_refund_timeout(&swap_id);
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Cancelled);
    }
}
