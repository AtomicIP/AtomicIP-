/// Snapshot tests for the Atomic Swap contract.
///
/// These tests verify that storage layout snapshots for `SwapRecord` and
/// related types remain stable across upgrades. If a snapshot changes
/// without an accompanying changelog entry, CI will fail.
#[cfg(test)]
mod snapshot_tests {
    use super::*;
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _,
        token::StellarAssetClient,
        Address, Bytes, BytesN, Env, Vec,
    };

    #[test]
    fn snapshot_swap_record_pending() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[1u8; 32]);
        let blinding = BytesN::from_array(&env, &[2u8; 32]);
        let mut preimage = Bytes::new(&env);
        preimage.append(&Bytes::from(secret));
        preimage.append(&Bytes::from(blinding));
        let hash = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &hash, &0u32);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        StellarAssetClient::new(&env, &token).mint(&buyer, &1000);
        let swap_id = client.initiate_swap(
            &token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Pending);
        assert_eq!(swap.price, 1000);
        assert_eq!(swap.seller, seller);
        assert_eq!(swap.buyer, buyer);
        assert_eq!(swap.token, token);
        assert_eq!(swap.ip_id, ip_id);
        assert_eq!(swap.quantity, 1);
        assert_eq!(swap.paid_amount, 0);
        assert!(!swap.is_installment);
        assert_eq!(swap.collateral_amount, 0);
        assert_eq!(swap.insurance_premium, 0);
        assert!(!swap.insurance_enabled);
        assert_eq!(swap.required_approvals, 0);
        assert!(swap.referrer.is_none());
        assert!(swap.escrow_agent.is_none());
        assert!(swap.conditions.is_empty());
        assert_eq!(swap.accept_timestamp, 0);
        assert_eq!(swap.dispute_timestamp, 0);
        assert!(swap.arbitrator.is_none());
    }

    #[test]
    fn snapshot_swap_record_accepted() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[1u8; 32]);
        let blinding = BytesN::from_array(&env, &[2u8; 32]);
        let mut preimage = Bytes::new(&env);
        preimage.append(&Bytes::from(secret));
        preimage.append(&Bytes::from(blinding));
        let hash = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &hash, &0u32);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        StellarAssetClient::new(&env, &token).mint(&buyer, &1000);
        let swap_id = client.initiate_swap(
            &token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Accepted);
        assert_eq!(swap.price, 1000);
        assert!(swap.accept_timestamp > 0);
    }

    #[test]
    fn snapshot_swap_record_completed() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[1u8; 32]);
        let blinding = BytesN::from_array(&env, &[2u8; 32]);
        let mut preimage = Bytes::new(&env);
        preimage.append(&Bytes::from(secret));
        preimage.append(&Bytes::from(blinding));
        let hash = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &hash, &0u32);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        StellarAssetClient::new(&env, &token).mint(&buyer, &1000);
        let swap_id = client.initiate_swap(
            &token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &secret, &blinding);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Completed);
        assert_eq!(swap.price, 1000);
    }

    #[test]
    fn snapshot_protocol_config_defaults() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&Address::generate(&env));

        let config = client.get_protocol_config();
        assert_eq!(config.protocol_fee_bps, 250);
        assert_eq!(config.referral_fee_bps, 100);
        assert_eq!(config.dispute_window_seconds, 86400);
        assert_eq!(config.dispute_timeout_secs, 604800);
        assert_eq!(config.arbitration_timeout_seconds, 1_209_600);
    }

    #[test]
    fn snapshot_swap_history_preserves_entries() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[1u8; 32]);
        let blinding = BytesN::from_array(&env, &[2u8; 32]);
        let mut preimage = Bytes::new(&env);
        preimage.append(&Bytes::from(secret));
        preimage.append(&Bytes::from(blinding));
        let hash = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &hash, &0u32);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        StellarAssetClient::new(&env, &token).mint(&buyer, &1000);
        let swap_id = client.initiate_swap(
            &token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );

        let history = client.get_swap_history(&swap_id);
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).unwrap().status, SwapStatus::Pending);
    }

    #[test]
    fn snapshot_arbitrator_committee_storage() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[1u8; 32]);
        let blinding = BytesN::from_array(&env, &[2u8; 32]);
        let mut preimage = Bytes::new(&env);
        preimage.append(&Bytes::from(secret));
        preimage.append(&Bytes::from(blinding));
        let hash = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &hash, &0u32);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        StellarAssetClient::new(&env, &token).mint(&buyer, &1000);
        let swap_id = client.initiate_swap(
            &token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);

        let signers = Vec::from_array(
            &env,
            [
                Address::generate(&env),
                Address::generate(&env),
                Address::generate(&env),
            ],
        );
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);

        let committee = env
            .storage()
            .persistent()
            .get::<DataKey, ArbitratorCommittee>(&DataKey::ArbitratorCommittee(swap_id))
            .unwrap();
        assert_eq!(committee.signers.len(), 3);
        assert_eq!(committee.threshold, 2);
    }
}
