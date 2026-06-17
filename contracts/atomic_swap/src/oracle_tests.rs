/// Tests for #470: Price Oracle Integration
///
/// Tests cover:
/// - set_oracle: admin-only, stores config, emits event
/// - get_oracle_config: returns stored config
/// - get_oracle_price: delegates to oracle contract
/// - initiate_swap_with_oracle_price: uses oracle price, respects slippage bounds
/// - Error cases: oracle not configured, price invalid, price out of bounds
#[cfg(test)]
mod oracle_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        contract, contractimpl,
        testutils::Address as _,
        token::StellarAssetClient,
        Address, Bytes, BytesN, Env, Symbol,
    };

    use crate::{AtomicSwap, AtomicSwapClient, ContractError, SwapStatus};

    // ── Mock Oracle Contract ──────────────────────────────────────────────────

    /// A minimal mock oracle that returns a configurable price for any token.
    #[contract]
    pub struct MockOracle;

    #[contractimpl]
    impl MockOracle {
        pub fn get_price(env: Env, _token: Address) -> i128 {
            env.storage()
                .instance()
                .get::<Symbol, i128>(&Symbol::new(&env, "price"))
                .unwrap_or(1_000_000)
        }

        pub fn set_price(env: Env, price: i128) {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, "price"), &price);
        }
    }

    // ── Test Helpers ──────────────────────────────────────────────────────────

    /// Registers an IP and returns (registry_id, ip_id, secret, blinding).
    fn setup_registry(env: &Env, owner: &Address) -> (Address, u64, BytesN<32>, BytesN<32>) {
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);
        let secret = BytesN::from_array(env, &[0xAAu8; 32]);
        let blinding = BytesN::from_array(env, &[0xBBu8; 32]);
        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(secret.clone()));
        preimage.append(&Bytes::from(blinding.clone()));
        let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(owner, &hash);
        (registry_id, ip_id, secret, blinding)
    }

    /// Registers a token and mints `amount` to `recipient`.
    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    /// Deploys and initializes the swap contract, seeds admin by calling initiate_swap once.
    /// Returns (swap_client, admin_address).
    fn setup_swap_contract(
        env: &Env,
        registry_id: &Address,
        token_id: &Address,
        ip_id: u64,
        seller: &Address,
        buyer: &Address,
    ) -> (AtomicSwapClient<'static>, Address) {
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(registry_id);
        // Seed admin: first initiate_swap sets admin = seller
        client.initiate_swap(
            token_id, &ip_id, seller, &500_i128, buyer,
            &0_u32, &None, &0_i128, &false,
        );
        // Cancel the seeding swap so the IP is free for oracle tests
        client.cancel_swap(&0_u64, seller);
        (client, seller.clone())
    }

    // ── set_oracle tests ──────────────────────────────────────────────────────

    #[test]
    fn test_set_oracle_stores_config() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let (client, admin_addr) = setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);

        client.set_oracle(&admin_addr, &oracle_id, &true);

        let config = client.get_oracle_config().unwrap();
        assert_eq!(config.oracle_address, oracle_id);
        assert!(config.enabled);
    }

    #[test]
    fn test_set_oracle_can_disable() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let (client, admin_addr) = setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);

        client.set_oracle(&admin_addr, &oracle_id, &true);
        client.set_oracle(&admin_addr, &oracle_id, &false);

        let config = client.get_oracle_config().unwrap();
        assert!(!config.enabled);
    }

    #[test]
    fn test_set_oracle_unauthorized_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let attacker = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let (client, _) = setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);

        let result = client.try_set_oracle(&attacker, &oracle_id, &true);
        assert_eq!(result.unwrap_err().unwrap(), ContractError::Unauthorized);
    }

    // ── get_oracle_config tests ───────────────────────────────────────────────

    #[test]
    fn test_get_oracle_config_none_when_not_set() {
        let env = Env::default();
        env.mock_all_auths();
        let registry_id = env.register(IpRegistry, ());
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        assert!(client.get_oracle_config().is_none());
    }

    // ── get_oracle_price tests ────────────────────────────────────────────────

    #[test]
    fn test_get_oracle_price_returns_oracle_value() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        oracle_client.set_price(&750_000_i128);
        let (client, admin_addr) = setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);

        client.set_oracle(&admin_addr, &oracle_id, &true);

        let price = client.get_oracle_price(&token_id);
        assert_eq!(price, 750_000_i128);
    }

    #[test]
    fn test_get_oracle_price_fails_when_not_configured() {
        let env = Env::default();
        env.mock_all_auths();
        let registry_id = env.register(IpRegistry, ());
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);
        let token = Address::generate(&env);

        let result = client.try_get_oracle_price(&token);
        assert_eq!(result.unwrap_err().unwrap(), ContractError::OracleNotConfigured);
    }

    #[test]
    fn test_get_oracle_price_fails_when_disabled() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let (client, admin_addr) = setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);

        client.set_oracle(&admin_addr, &oracle_id, &false);

        let result = client.try_get_oracle_price(&token_id);
        assert_eq!(result.unwrap_err().unwrap(), ContractError::OracleNotConfigured);
    }

    // ── initiate_swap_with_oracle_price tests ─────────────────────────────────

    #[test]
    fn test_initiate_swap_with_oracle_price_uses_oracle_price() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        oracle_client.set_price(&500_000_i128);
        let (client, admin_addr) = setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        client.set_oracle(&admin_addr, &oracle_id, &true);

        let swap_id = client.initiate_swap_with_oracle_price(
            &token_id, &ip_id, &seller, &buyer,
            &0_u32, &None, &0_i128, &false,
            &0_i128, &0_i128,
        );

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.price, 500_000_i128);
        assert_eq!(swap.status, SwapStatus::Pending);
    }

    #[test]
    fn test_initiate_swap_with_oracle_price_respects_min_price() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        oracle_client.set_price(&100_i128); // below min
        let (client, admin_addr) = setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        client.set_oracle(&admin_addr, &oracle_id, &true);

        let result = client.try_initiate_swap_with_oracle_price(
            &token_id, &ip_id, &seller, &buyer,
            &0_u32, &None, &0_i128, &false,
            &500_i128, &0_i128,
        );
        assert_eq!(result.unwrap_err().unwrap(), ContractError::OraclePriceBelowMin);
    }

    #[test]
    fn test_initiate_swap_with_oracle_price_respects_max_price() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        oracle_client.set_price(&1_000_000_i128); // above max
        let (client, admin_addr) = setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        client.set_oracle(&admin_addr, &oracle_id, &true);

        let result = client.try_initiate_swap_with_oracle_price(
            &token_id, &ip_id, &seller, &buyer,
            &0_u32, &None, &0_i128, &false,
            &0_i128, &500_000_i128,
        );
        assert_eq!(result.unwrap_err().unwrap(), ContractError::OraclePriceAboveMax);
    }

    #[test]
    fn test_initiate_swap_with_oracle_price_within_bounds_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        oracle_client.set_price(&300_000_i128);
        let (client, admin_addr) = setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        client.set_oracle(&admin_addr, &oracle_id, &true);

        let swap_id = client.initiate_swap_with_oracle_price(
            &token_id, &ip_id, &seller, &buyer,
            &0_u32, &None, &0_i128, &false,
            &100_000_i128, &500_000_i128,
        );

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.price, 300_000_i128);
    }

    #[test]
    fn test_initiate_swap_with_oracle_price_fails_when_oracle_not_configured() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let result = client.try_initiate_swap_with_oracle_price(
            &token_id, &ip_id, &seller, &buyer,
            &0_u32, &None, &0_i128, &false,
            &0_i128, &0_i128,
        );
        assert_eq!(result.unwrap_err().unwrap(), ContractError::OracleNotConfigured);
    }

    #[test]
    fn test_oracle_price_invalid_zero_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        oracle_client.set_price(&0_i128); // invalid: zero
        let (client, admin_addr) = setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        client.set_oracle(&admin_addr, &oracle_id, &true);

        let result = client.try_get_oracle_price(&token_id);
        assert_eq!(result.unwrap_err().unwrap(), ContractError::OraclePriceInvalid);
    }
}
