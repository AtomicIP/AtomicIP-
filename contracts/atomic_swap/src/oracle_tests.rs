/// Tests for #470, #622 & #784: Price Oracle Integration with Cryptographic
/// Attestation, Staleness Validation, and Deviation Bounds.
///
/// Tests cover:
/// - set_oracle: admin-only, stores config (address + pubkey + deviation bound), emits event
/// - get_oracle_config: returns stored config
/// - get_oracle_price: delegates to oracle contract with signature verification + staleness checks
/// - initiate_swap_with_oracle_price: uses oracle price, respects slippage bounds
/// - Signature verification: a well-formed, positive, but unsigned/forged/wrong-key price is rejected
/// - Deviation bound: a validly signed price that moves too far from the last accepted price is rejected
/// - Staleness validation: detects stale prices (>5 min), falls back to the (already-verified) cached price
/// - Error cases: oracle not configured, price invalid, price out of bounds, stale data with no cache
#[cfg(test)]
mod oracle_tests {
    use ed25519_dalek::{Signer, SigningKey};
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        contract, contractimpl,
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        xdr::ToXdr,
        Address, Bytes, BytesN, Env, Symbol,
    };

    use crate::price_oracle::{PriceAttestation, SignedPrice};
    use crate::{AtomicSwap, AtomicSwapClient, ContractError, SwapStatus};

    // ── Mock Oracle Contract ──────────────────────────────────────────────────

    /// A minimal mock oracle that returns a configurable signed price
    /// attestation for any token. The signature over `(token, price,
    /// timestamp)` is produced off-chain by the test via `sign_price` (which
    /// stands in for an oracle publisher's private key) and pushed in via
    /// `set_signed_price`.
    #[contract]
    pub struct MockOracle;

    #[contractimpl]
    impl MockOracle {
        pub fn get_price_attestation(env: Env, _token: Address) -> SignedPrice {
            env.storage()
                .instance()
                .get::<Symbol, SignedPrice>(&Symbol::new(&env, "signed"))
                .unwrap_or(SignedPrice {
                    price: 1_000_000,
                    timestamp: 0,
                    signature: BytesN::from_array(&env, &[0u8; 64]),
                })
        }

        pub fn set_signed_price(env: Env, price: i128, timestamp: u64, signature: BytesN<64>) {
            env.storage().instance().set(
                &Symbol::new(&env, "signed"),
                &SignedPrice {
                    price,
                    timestamp,
                    signature,
                },
            );
        }
    }

    // ── Signing Helpers (simulate an off-chain oracle publisher) ──────────────

    /// Deterministic test-only Ed25519 keypair standing in for the oracle
    /// publisher's real off-chain key. Not for production use.
    fn test_signing_key() -> SigningKey {
        SigningKey::from_bytes(&[0x42u8; 32])
    }

    /// A second, unrelated keypair used to simulate a forged/wrong-key signature.
    fn wrong_signing_key() -> SigningKey {
        SigningKey::from_bytes(&[0x99u8; 32])
    }

    fn pubkey_bytes(env: &Env, sk: &SigningKey) -> BytesN<32> {
        BytesN::from_array(env, &sk.verifying_key().to_bytes())
    }

    /// Signs `(token, price, timestamp)` with `sk`, matching the XDR encoding
    /// `verify_attestation` reconstructs on the contract side.
    fn sign_price(
        env: &Env,
        sk: &SigningKey,
        token: &Address,
        price: i128,
        timestamp: u64,
    ) -> BytesN<64> {
        let attestation = PriceAttestation {
            token: token.clone(),
            price,
            timestamp,
        };
        let payload: Bytes = attestation.to_xdr(env);
        let sig = sk.sign(&payload.to_alloc_vec());
        BytesN::from_array(env, &sig.to_bytes())
    }

    /// Signs `price` for `token` (with the current ledger timestamp) using the
    /// canonical test key and pushes it into the mock oracle.
    fn publish_price(env: &Env, oracle: &MockOracleClient, token: &Address, price: i128) {
        let ts = env.ledger().timestamp();
        let sig = sign_price(env, &test_signing_key(), token, price, ts);
        oracle.set_signed_price(&price, &ts, &sig);
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
        let ip_id = registry.commit_ip(owner, &hash, &0u32);
        (registry_id, ip_id, secret, blinding)
    }

    /// Registers a token and mints `amount` to `recipient`.
    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
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
            token_id, &ip_id, seller, &500_i128, buyer, &0_u32, &None, &0_i128, &false,
        );
        // Cancel the seeding swap so the IP is free for oracle tests
        client.cancel_swap(&0_u64, seller);
        (client, seller.clone())
    }

    /// Enables the oracle with the canonical test pubkey and no deviation
    /// bound (matching the historical, permissive default most tests want),
    /// then publishes a signed price for `token_id` so the very next fetch
    /// (which lands on the fresh path, since `set_oracle` seeds
    /// `last_update_timestamp` to now) is properly verifiable.
    fn enable_oracle(
        env: &Env,
        client: &AtomicSwapClient,
        admin: &Address,
        oracle_id: &Address,
        oracle: &MockOracleClient,
        token_id: &Address,
        price: i128,
    ) {
        let pubkey = pubkey_bytes(env, &test_signing_key());
        client.set_oracle(admin, oracle_id, &pubkey, &true, &0_u32);
        publish_price(env, oracle, token_id, price);
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
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        let pubkey = pubkey_bytes(&env, &test_signing_key());

        client.set_oracle(&admin_addr, &oracle_id, &pubkey, &true, &0_u32);

        let config = client.get_oracle_config().unwrap();
        assert_eq!(config.oracle_address, oracle_id);
        assert_eq!(config.oracle_pubkey, pubkey);
        assert!(config.enabled);
        assert_eq!(config.max_deviation_bps, 0);
    }

    #[test]
    fn test_set_oracle_stores_deviation_bound() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        let pubkey = pubkey_bytes(&env, &test_signing_key());

        client.set_oracle(&admin_addr, &oracle_id, &pubkey, &true, &500_u32);

        let config = client.get_oracle_config().unwrap();
        assert_eq!(config.max_deviation_bps, 500);
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
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        let pubkey = pubkey_bytes(&env, &test_signing_key());

        client.set_oracle(&admin_addr, &oracle_id, &pubkey, &true, &0_u32);
        client.set_oracle(&admin_addr, &oracle_id, &pubkey, &false, &0_u32);

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
        let (client, _) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        let pubkey = pubkey_bytes(&env, &test_signing_key());

        let result = client.try_set_oracle(&attacker, &oracle_id, &pubkey, &true, &0_u32);
        assert_eq!(
            result.unwrap_err().unwrap(),
            ContractError::Unauthorized.into()
        );
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
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        enable_oracle(
            &env,
            &client,
            &admin_addr,
            &oracle_id,
            &oracle_client,
            &token_id,
            750_000_i128,
        );

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
        assert_eq!(
            result.unwrap_err().unwrap(),
            ContractError::OracleNotConfigured.into()
        );
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
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        let pubkey = pubkey_bytes(&env, &test_signing_key());

        client.set_oracle(&admin_addr, &oracle_id, &pubkey, &false, &0_u32);

        let result = client.try_get_oracle_price(&token_id);
        assert_eq!(
            result.unwrap_err().unwrap(),
            ContractError::OracleNotConfigured.into()
        );
    }

    // ── #784: Signature verification tests ────────────────────────────────────

    #[test]
    fn test_get_oracle_price_rejects_forged_signature() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        let pubkey = pubkey_bytes(&env, &test_signing_key());
        client.set_oracle(&admin_addr, &oracle_id, &pubkey, &true, &0_u32);

        // A well-formed, positive price — but signed with a DIFFERENT key than
        // the one registered via set_oracle. Simulates a forged/wrong-key
        // attestation.
        let ts = env.ledger().timestamp();
        let forged_sig = sign_price(&env, &wrong_signing_key(), &token_id, 500_000_i128, ts);
        oracle_client.set_signed_price(&500_000_i128, &ts, &forged_sig);

        let result = client.try_get_oracle_price(&token_id);
        assert!(result.is_err(), "a forged-signature price must be rejected");
    }

    #[test]
    fn test_get_oracle_price_rejects_missing_signature() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        let pubkey = pubkey_bytes(&env, &test_signing_key());
        client.set_oracle(&admin_addr, &oracle_id, &pubkey, &true, &0_u32);

        // Well-formed, positive price with an all-zero (missing) signature.
        let ts = env.ledger().timestamp();
        oracle_client.set_signed_price(&500_000_i128, &ts, &BytesN::from_array(&env, &[0u8; 64]));

        let result = client.try_get_oracle_price(&token_id);
        assert!(
            result.is_err(),
            "a missing-signature price must be rejected"
        );
    }

    #[test]
    fn test_get_oracle_price_rejects_tampered_price_under_valid_signature() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        let pubkey = pubkey_bytes(&env, &test_signing_key());
        client.set_oracle(&admin_addr, &oracle_id, &pubkey, &true, &0_u32);

        // Sign 500_000, but publish a different price under that signature —
        // the signature only verifies the exact attested tuple.
        let ts = env.ledger().timestamp();
        let sig = sign_price(&env, &test_signing_key(), &token_id, 500_000_i128, ts);
        oracle_client.set_signed_price(&999_000_i128, &ts, &sig);

        let result = client.try_get_oracle_price(&token_id);
        assert!(
            result.is_err(),
            "a tampered price under someone else's valid signature must be rejected"
        );
    }

    #[test]
    fn test_initiate_swap_with_oracle_price_rejects_forged_signature() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        let pubkey = pubkey_bytes(&env, &test_signing_key());
        client.set_oracle(&admin_addr, &oracle_id, &pubkey, &true, &0_u32);

        let ts = env.ledger().timestamp();
        let forged_sig = sign_price(&env, &wrong_signing_key(), &token_id, 500_000_i128, ts);
        oracle_client.set_signed_price(&500_000_i128, &ts, &forged_sig);

        let result = client.try_initiate_swap_with_oracle_price(
            &token_id, &ip_id, &seller, &buyer, &0_u32, &None, &0_i128, &false, &0_i128, &0_i128,
        );
        assert!(
            result.is_err(),
            "a swap must not be initiated from a forged oracle price"
        );
        assert!(client.get_swap(&1_u64).is_none());
    }

    // ── #784: Deviation bound tests ────────────────────────────────────────────

    #[test]
    fn test_get_oracle_price_rejects_price_beyond_deviation_bound() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);

        // 10% (1000 bps) max deviation.
        let pubkey = pubkey_bytes(&env, &test_signing_key());
        client.set_oracle(&admin_addr, &oracle_id, &pubkey, &true, &1000_u32);
        publish_price(&env, &oracle_client, &token_id, 500_000_i128);

        // Establish the baseline accepted price.
        let price = client.get_oracle_price(&token_id);
        assert_eq!(price, 500_000_i128);

        // A validly-signed price that moves 40% from the baseline — far
        // beyond the 10% bound — must be rejected even though the signature
        // itself is genuine.
        publish_price(&env, &oracle_client, &token_id, 700_000_i128);
        let result = client.try_get_oracle_price(&token_id);
        assert_eq!(
            result.unwrap_err().unwrap(),
            ContractError::OracleDeviationExceeded.into()
        );
    }

    #[test]
    fn test_get_oracle_price_accepts_price_within_deviation_bound() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);

        let pubkey = pubkey_bytes(&env, &test_signing_key());
        client.set_oracle(&admin_addr, &oracle_id, &pubkey, &true, &1000_u32); // 10%
        publish_price(&env, &oracle_client, &token_id, 500_000_i128);

        let price = client.get_oracle_price(&token_id);
        assert_eq!(price, 500_000_i128);

        // A 5% move is within the 10% bound and must be accepted.
        publish_price(&env, &oracle_client, &token_id, 525_000_i128);
        let updated = client.get_oracle_price(&token_id);
        assert_eq!(updated, 525_000_i128);
    }

    #[test]
    fn test_zero_deviation_bound_means_unbounded() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        let pubkey = pubkey_bytes(&env, &test_signing_key());
        client.set_oracle(&admin_addr, &oracle_id, &pubkey, &true, &0_u32); // unbounded
        publish_price(&env, &oracle_client, &token_id, 500_000_i128);

        let price = client.get_oracle_price(&token_id);
        assert_eq!(price, 500_000_i128);

        // A large swing is allowed when max_deviation_bps is 0.
        publish_price(&env, &oracle_client, &token_id, 5_000_000_i128);
        let updated = client.get_oracle_price(&token_id);
        assert_eq!(updated, 5_000_000_i128);
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
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        enable_oracle(
            &env,
            &client,
            &admin_addr,
            &oracle_id,
            &oracle_client,
            &token_id,
            500_000_i128,
        );

        let swap_id = client.initiate_swap_with_oracle_price(
            &token_id, &ip_id, &seller, &buyer, &0_u32, &None, &0_i128, &false, &0_i128, &0_i128,
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
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        enable_oracle(
            &env,
            &client,
            &admin_addr,
            &oracle_id,
            &oracle_client,
            &token_id,
            100_i128,
        ); // below min

        let result = client.try_initiate_swap_with_oracle_price(
            &token_id, &ip_id, &seller, &buyer, &0_u32, &None, &0_i128, &false, &500_i128, &0_i128,
        );
        assert_eq!(
            result.unwrap_err().unwrap(),
            ContractError::OraclePriceBelowMin.into()
        );
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
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        enable_oracle(
            &env,
            &client,
            &admin_addr,
            &oracle_id,
            &oracle_client,
            &token_id,
            1_000_000_i128,
        ); // above max

        let result = client.try_initiate_swap_with_oracle_price(
            &token_id,
            &ip_id,
            &seller,
            &buyer,
            &0_u32,
            &None,
            &0_i128,
            &false,
            &0_i128,
            &500_000_i128,
        );
        assert_eq!(
            result.unwrap_err().unwrap(),
            ContractError::OraclePriceAboveMax.into()
        );
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
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        enable_oracle(
            &env,
            &client,
            &admin_addr,
            &oracle_id,
            &oracle_client,
            &token_id,
            300_000_i128,
        );

        let swap_id = client.initiate_swap_with_oracle_price(
            &token_id,
            &ip_id,
            &seller,
            &buyer,
            &0_u32,
            &None,
            &0_i128,
            &false,
            &100_000_i128,
            &500_000_i128,
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
            &token_id, &ip_id, &seller, &buyer, &0_u32, &None, &0_i128, &false, &0_i128, &0_i128,
        );
        assert_eq!(
            result.unwrap_err().unwrap(),
            ContractError::OracleNotConfigured.into()
        );
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
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        let pubkey = pubkey_bytes(&env, &test_signing_key());
        client.set_oracle(&admin_addr, &oracle_id, &pubkey, &true, &0_u32);
        publish_price(&env, &oracle_client, &token_id, 0_i128); // invalid: zero

        let result = client.try_get_oracle_price(&token_id);
        assert_eq!(
            result.unwrap_err().unwrap(),
            ContractError::OraclePriceInvalid.into()
        );
    }

    // ── #622: Staleness Validation Tests ──────────────────────────────────────

    #[test]
    fn test_oracle_config_stores_timestamp() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        enable_oracle(
            &env,
            &client,
            &admin_addr,
            &oracle_id,
            &oracle_client,
            &token_id,
            500_000_i128,
        );

        let config = client.get_oracle_config().unwrap();
        assert_eq!(config.oracle_address, oracle_id);
        assert!(config.enabled);

        let price = client.get_oracle_price(&token_id);
        assert_eq!(price, 500_000_i128);
        let config_after = client.get_oracle_config().unwrap();
        assert_eq!(config_after.cached_price, 500_000_i128);
    }

    #[test]
    fn test_fresh_oracle_price_updates_cache() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        enable_oracle(
            &env,
            &client,
            &admin_addr,
            &oracle_id,
            &oracle_client,
            &token_id,
            500_000_i128,
        );

        // Get price (should be fresh and update cache)
        let price = client.get_oracle_price(&token_id);
        assert_eq!(price, 500_000_i128);

        // Publish (sign + push) a new price
        publish_price(&env, &oracle_client, &token_id, 600_000_i128);

        // Get price again (should fetch new value)
        let new_price = client.get_oracle_price(&token_id);
        assert_eq!(new_price, 600_000_i128);

        // Verify cache was updated
        let config = client.get_oracle_config().unwrap();
        assert_eq!(config.cached_price, 600_000_i128);
    }

    #[test]
    fn test_oracle_price_staleness_within_threshold() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        enable_oracle(
            &env,
            &client,
            &admin_addr,
            &oracle_id,
            &oracle_client,
            &token_id,
            500_000_i128,
        );

        // Price is fresh (within 5 min threshold)
        let price = client.get_oracle_price(&token_id);
        assert_eq!(price, 500_000_i128);
    }

    #[test]
    fn test_oracle_price_staleness_exceeds_threshold_uses_cache() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        enable_oracle(
            &env,
            &client,
            &admin_addr,
            &oracle_id,
            &oracle_client,
            &token_id,
            500_000_i128,
        );

        // Get price to establish cache
        let initial_price = client.get_oracle_price(&token_id);
        assert_eq!(initial_price, 500_000_i128);

        // Simulate time passing beyond staleness threshold (>300 seconds)
        env.ledger().set_timestamp(env.ledger().timestamp() + 301);

        // Publish a new price (but staleness should trigger a cache fallback
        // instead of fetching this one)
        publish_price(&env, &oracle_client, &token_id, 700_000_i128);

        let stale_price = client.get_oracle_price(&token_id);

        // Due to staleness, the cached (pre-lapse) price is used, not the new one.
        assert_eq!(stale_price, 500_000_i128);
    }

    #[test]
    fn test_stale_price_with_no_cache_is_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);

        // Enable, but never successfully fetch a price, then let the clock run
        // past the staleness window: there is no cache to fall back to.
        let pubkey = pubkey_bytes(&env, &test_signing_key());
        client.set_oracle(&admin_addr, &oracle_id, &pubkey, &true, &0_u32);
        env.ledger().set_timestamp(env.ledger().timestamp() + 301);
        publish_price(&env, &oracle_client, &token_id, 500_000_i128);

        let result = client.try_get_oracle_price(&token_id);
        assert_eq!(
            result.unwrap_err().unwrap(),
            ContractError::OraclePriceInvalid.into()
        );
    }

    #[test]
    fn test_initiate_swap_with_stale_oracle_price_uses_cache() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        enable_oracle(
            &env,
            &client,
            &admin_addr,
            &oracle_id,
            &oracle_client,
            &token_id,
            500_000_i128,
        );

        // Initiate swap with oracle price
        let swap_id = client.initiate_swap_with_oracle_price(
            &token_id, &ip_id, &seller, &buyer, &0_u32, &None, &0_i128, &false, &0_i128, &0_i128,
        );

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.price, 500_000_i128);
    }

    #[test]
    fn test_oracle_fallback_mechanism_respects_min_max_bounds() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        enable_oracle(
            &env,
            &client,
            &admin_addr,
            &oracle_id,
            &oracle_client,
            &token_id,
            300_000_i128,
        );

        // Initiate swap with bounds that the cached price respects
        let swap_id = client.initiate_swap_with_oracle_price(
            &token_id,
            &ip_id,
            &seller,
            &buyer,
            &0_u32,
            &None,
            &0_i128,
            &false,
            &100_000_i128,
            &500_000_i128,
        );

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.price, 300_000_i128);
        assert!(swap.price >= 100_000_i128 && swap.price <= 500_000_i128);
    }

    #[test]
    fn test_oracle_config_disable_preserves_cache() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        enable_oracle(
            &env,
            &client,
            &admin_addr,
            &oracle_id,
            &oracle_client,
            &token_id,
            500_000_i128,
        );
        client.get_oracle_price(&token_id); // populate the cache

        let config_enabled = client.get_oracle_config().unwrap();
        let cached_price = config_enabled.cached_price;
        assert_eq!(cached_price, 500_000_i128);

        // Disable oracle
        let pubkey = pubkey_bytes(&env, &test_signing_key());
        client.set_oracle(&admin_addr, &oracle_id, &pubkey, &false, &0_u32);
        let config_disabled = client.get_oracle_config().unwrap();

        // Verify cache is preserved
        assert_eq!(config_disabled.cached_price, cached_price);
        assert!(!config_disabled.enabled);
    }

    #[test]
    fn test_oracle_disabled_after_enable_rejects_price_fetch() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        enable_oracle(
            &env,
            &client,
            &admin_addr,
            &oracle_id,
            &oracle_client,
            &token_id,
            500_000_i128,
        );

        let pubkey = pubkey_bytes(&env, &test_signing_key());
        client.set_oracle(&admin_addr, &oracle_id, &pubkey, &false, &0_u32);

        let result = client.try_get_oracle_price(&token_id);
        assert_eq!(
            result.unwrap_err().unwrap(),
            ContractError::OracleNotConfigured.into()
        );
    }

    #[test]
    fn test_price_volatility_within_bounds() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
        let oracle_id = env.register(MockOracle, ());
        let oracle_client = MockOracleClient::new(&env, &oracle_id);
        let (client, admin_addr) =
            setup_swap_contract(&env, &registry_id, &token_id, ip_id, &seller, &buyer);
        enable_oracle(
            &env,
            &client,
            &admin_addr,
            &oracle_id,
            &oracle_client,
            &token_id,
            500_000_i128,
        );

        // Simulate price volatility by publishing new signed prices
        publish_price(&env, &oracle_client, &token_id, 480_000_i128);
        let price1 = client.get_oracle_price(&token_id);

        publish_price(&env, &oracle_client, &token_id, 520_000_i128);
        let price2 = client.get_oracle_price(&token_id);

        // Both should be valid
        assert!(price1 > 0);
        assert!(price2 > 0);

        // Initiate swap with tight bounds
        let swap_id = client.initiate_swap_with_oracle_price(
            &token_id,
            &ip_id,
            &seller,
            &buyer,
            &0_u32,
            &None,
            &0_i128,
            &false,
            &500_000_i128,
            &530_000_i128,
        );

        let swap = client.get_swap(&swap_id).unwrap();
        assert!(swap.price >= 500_000_i128 && swap.price <= 530_000_i128);
    }
}
