#[cfg(test)]
mod tests {
    use atomic_swap::{AtomicSwap, AtomicSwapClient, SwapStatus};
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _,
        token::StellarAssetClient,
        Address, BytesN, Env,
    };

    fn setup_registry(env: &Env, owner: &Address) -> (Address, u64) {
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);
        let secret = BytesN::from_array(env, &[2u8; 32]);
        let blinding = BytesN::from_array(env, &[3u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(owner, &commitment_hash);
        (registry_id, ip_id)
    }

    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    fn setup_swap(env: &Env) -> (AtomicSwapClient, u64, Address, Address) {
        let seller = Address::generate(env);
        let buyer = Address::generate(env);
        let admin = Address::generate(env);
        let (registry_id, ip_id) = setup_registry(env, &seller);
        let token_id = setup_token(env, &admin, &buyer, 10_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(&token_id, &ip_id, &seller, &1000_i128, &buyer, &0_u32, &None);
        (client, swap_id, seller, buyer)
    }

    // ── renegotiation ─────────────────────────────────────────────────────────

    #[test]
    fn test_propose_renegotiation() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        // Seller proposes a new price
        client.renegotiate_swap(&swap_id, &800_i128);

        // Swap price unchanged until buyer accepts
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.price, 1000);
        assert_eq!(swap.status, SwapStatus::Pending);
    }

    #[test]
    fn test_accept_renegotiation() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        client.renegotiate_swap(&swap_id, &750_i128);
        client.accept_renegotiation(&swap_id);

        // Price updated to the negotiated value
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.price, 750);
        assert_eq!(swap.status, SwapStatus::Pending);
    }

    #[test]
    fn test_renegotiation_then_accept_swap_uses_new_price() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        client.renegotiate_swap(&swap_id, &600_i128);
        client.accept_renegotiation(&swap_id);
        // Buyer now accepts the swap at the renegotiated price
        client.accept_swap(&swap_id);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Accepted);
        assert_eq!(swap.price, 600);
    }

    #[test]
    #[should_panic]
    fn test_accept_renegotiation_without_offer_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        // No offer proposed — should panic
        client.accept_renegotiation(&swap_id);
    }

    #[test]
    #[should_panic]
    fn test_renegotiate_zero_price_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        client.renegotiate_swap(&swap_id, &0_i128);
    }

    #[test]
    fn test_renegotiate_overwrites_previous_offer() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, swap_id, _seller, _buyer) = setup_swap(&env);

        client.renegotiate_swap(&swap_id, &900_i128);
        // Seller changes their mind
        client.renegotiate_swap(&swap_id, &850_i128);
        client.accept_renegotiation(&swap_id);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.price, 850);
    }

    #[test]
    fn test_accept_swap_with_insurance() {
        // placeholder — insurance feature not yet implemented
    }

    #[test]
    fn test_initiate_swap_with_escrow() {
        // placeholder — escrow feature not yet implemented
    }

    #[test]
    fn test_escrow_release_funds() {
        // placeholder — escrow feature not yet implemented
    }

    #[test]
    fn test_accept_swap_conditional() {
        // placeholder — conditional acceptance not yet implemented
    }
}
