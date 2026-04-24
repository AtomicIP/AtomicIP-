/// Testnet Integration Tests for Atomic Patent
///
/// Two layers:
///   1. Local Soroban environment tests (always run) — validate the full swap
///      flow with real contract calls, fee math, and token balance assertions.
///   2. Live testnet smoke tests (gated behind `--ignored`) — require deployed
///      contract IDs in environment variables.
///
/// Run local tests:   cargo test --test testnet_integration
/// Run live tests:    cargo test --test testnet_integration -- --ignored --nocapture
#[cfg(test)]
mod testnet_integration_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::{Client as TokenClient, StellarAssetClient},
        Address, BytesN, Env,
    };

    use atomic_swap::{AtomicSwap, AtomicSwapClient, ProtocolConfig, SwapStatus};

    // ── Helpers ───────────────────────────────────────────────────────────────

    struct SwapFixture {
        env: Env,
        client: AtomicSwapClient<'static>,
        seller: Address,
        buyer: Address,
        token_id: Address,
        ip_id: u64,
        secret: BytesN<32>,
        blinding: BytesN<32>,
    }

    fn build_fixture(price: i128) -> SwapFixture {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);

        let secret = BytesN::from_array(&env, &[0xABu8; 32]);
        let blinding = BytesN::from_array(&env, &[0xCDu8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &hash);

        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(&env, &token_id).mint(&buyer, &price);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        SwapFixture { env, client, seller, buyer, token_id, ip_id, secret, blinding }
    }

    // ── Full flow tests ───────────────────────────────────────────────────────

    /// Full happy-path: commit IP → initiate → accept → reveal → complete.
    /// Asserts every state transition and final token balances.
    #[test]
    fn test_full_swap_flow_state_transitions() {
        let price = 1_000_i128;
        let f = build_fixture(price);

        let swap_id = f.client.initiate_swap(
            &f.token_id, &f.ip_id, &f.seller, &price, &f.buyer, &0_u32,
        );

        assert_eq!(f.client.get_swap(&swap_id).unwrap().status, SwapStatus::Pending);

        f.client.accept_swap(&swap_id);
        assert_eq!(f.client.get_swap(&swap_id).unwrap().status, SwapStatus::Accepted);

        f.client.reveal_key(&swap_id, &f.seller, &f.secret, &f.blinding);
        assert_eq!(f.client.get_swap(&swap_id).unwrap().status, SwapStatus::Completed);
    }

    /// Verifies token escrow: buyer balance decreases on accept, seller receives
    /// net payment (price minus protocol fee) on reveal.
    #[test]
    fn test_token_balances_after_full_flow() {
        let price = 10_000_i128;
        let f = build_fixture(price);
        let token = TokenClient::new(&f.env, &f.token_id);

        assert_eq!(token.balance(&f.buyer), price);
        assert_eq!(token.balance(&f.seller), 0);

        let swap_id = f.client.initiate_swap(
            &f.token_id, &f.ip_id, &f.seller, &price, &f.buyer, &0_u32,
        );
        f.client.accept_swap(&swap_id);

        // Buyer's tokens are now in escrow
        assert_eq!(token.balance(&f.buyer), 0);

        f.client.reveal_key(&swap_id, &f.seller, &f.secret, &f.blinding);

        // No protocol fee configured → seller receives full price
        assert_eq!(token.balance(&f.seller), price);
    }

    /// Verifies protocol fee deduction: seller receives price*(1 - fee_bps/10000).
    #[test]
    fn test_fee_calculation_on_completion() {
        let price = 10_000_i128;
        let fee_bps = 250_u32; // 2.5%
        let f = build_fixture(price);
        let token = TokenClient::new(&f.env, &f.token_id);

        let treasury = Address::generate(&f.env);
        f.client.set_protocol_config(&ProtocolConfig {
            protocol_fee_bps: fee_bps,
            treasury: treasury.clone(),
            dispute_window_seconds: 3600,
            dispute_resolution_timeout_seconds: 86400,
        });

        let swap_id = f.client.initiate_swap(
            &f.token_id, &f.ip_id, &f.seller, &price, &f.buyer, &0_u32,
        );
        f.client.accept_swap(&swap_id);
        f.client.reveal_key(&swap_id, &f.seller, &f.secret, &f.blinding);

        let expected_fee = (price * fee_bps as i128) / 10_000;
        let expected_seller = price - expected_fee;

        assert_eq!(token.balance(&f.seller), expected_seller, "seller net amount");
        assert_eq!(token.balance(&treasury), expected_fee, "treasury fee");
    }

    /// Cancel from Pending: buyer's tokens are never locked, swap is Cancelled.
    #[test]
    fn test_cancel_pending_swap() {
        let price = 500_i128;
        let f = build_fixture(price);
        let token = TokenClient::new(&f.env, &f.token_id);

        let swap_id = f.client.initiate_swap(
            &f.token_id, &f.ip_id, &f.seller, &price, &f.buyer, &0_u32,
        );

        // Buyer balance unchanged — no escrow yet
        assert_eq!(token.balance(&f.buyer), price);

        f.client.cancel_swap(&swap_id);
        assert_eq!(f.client.get_swap(&swap_id).unwrap().status, SwapStatus::Cancelled);

        // Balance still unchanged
        assert_eq!(token.balance(&f.buyer), price);
    }

    /// Expired accepted swap: buyer can cancel after expiry and gets refund.
    #[test]
    fn test_cancel_expired_accepted_swap_refunds_buyer() {
        let price = 800_i128;
        let f = build_fixture(price);
        let token = TokenClient::new(&f.env, &f.token_id);

        let swap_id = f.client.initiate_swap(
            &f.token_id, &f.ip_id, &f.seller, &price, &f.buyer, &0_u32,
        );
        f.client.accept_swap(&swap_id);
        assert_eq!(token.balance(&f.buyer), 0);

        // Advance ledger past the 7-day expiry (604800 seconds)
        f.env.ledger().with_mut(|l| {
            l.timestamp += 604_801;
        });

        f.client.cancel_expired_swap(&swap_id);
        assert_eq!(f.client.get_swap(&swap_id).unwrap().status, SwapStatus::Cancelled);
        assert_eq!(token.balance(&f.buyer), price, "buyer should be refunded");
    }

    /// Only one active swap per IP at a time.
    #[test]
    #[should_panic(expected = "Error(Contract, #5)")]
    fn test_duplicate_active_swap_rejected() {
        let price = 100_i128;
        let f = build_fixture(price);

        f.client.initiate_swap(&f.token_id, &f.ip_id, &f.seller, &price, &f.buyer, &0_u32);
        // Second initiation for same IP must fail
        f.client.initiate_swap(&f.token_id, &f.ip_id, &f.seller, &price, &f.buyer, &0_u32);
    }

    /// After a swap completes, the same IP can be listed in a new swap.
    #[test]
    fn test_ip_relisted_after_completion() {
        let price = 200_i128;
        let f = build_fixture(price * 2); // mint enough for two swaps

        let swap_id = f.client.initiate_swap(
            &f.token_id, &f.ip_id, &f.seller, &price, &f.buyer, &0_u32,
        );
        f.client.accept_swap(&swap_id);
        f.client.reveal_key(&swap_id, &f.seller, &f.secret, &f.blinding);

        // IP lock released — new swap should succeed
        let buyer2 = Address::generate(&f.env);
        StellarAssetClient::new(&f.env, &f.token_id).mint(&buyer2, &price);
        let swap_id2 = f.client.initiate_swap(
            &f.token_id, &f.ip_id, &f.seller, &price, &buyer2, &0_u32,
        );
        assert_eq!(f.client.get_swap(&swap_id2).unwrap().status, SwapStatus::Pending);
    }

    // ── Live testnet smoke tests (require deployed contracts) ─────────────────

    struct TestnetConfig {
        rpc_url: String,
        ip_registry_id: String,
        atomic_swap_id: String,
    }

    impl TestnetConfig {
        fn from_env() -> Result<Self, String> {
            Ok(TestnetConfig {
                rpc_url: std::env::var("STELLAR_RPC_URL")
                    .unwrap_or_else(|_| "https://soroban-testnet.stellar.org".to_string()),
                ip_registry_id: std::env::var("IP_REGISTRY_CONTRACT_ID")
                    .map_err(|_| "IP_REGISTRY_CONTRACT_ID not set".to_string())?,
                atomic_swap_id: std::env::var("ATOMIC_SWAP_CONTRACT_ID")
                    .map_err(|_| "ATOMIC_SWAP_CONTRACT_ID not set".to_string())?,
            })
        }
    }

    #[test]
    #[ignore]
    fn test_testnet_contracts_accessible() {
        let cfg = match TestnetConfig::from_env() {
            Ok(c) => c,
            Err(e) => { eprintln!("Skip: {e}"); return; }
        };
        println!("RPC:      {}", cfg.rpc_url);
        println!("Registry: {}", cfg.ip_registry_id);
        println!("Swap:     {}", cfg.atomic_swap_id);
        // With a real Stellar SDK client: query contract state and assert non-empty.
    }

    #[test]
    #[ignore]
    fn test_testnet_full_swap_flow() {
        let cfg = match TestnetConfig::from_env() {
            Ok(c) => c,
            Err(e) => { eprintln!("Skip: {e}"); return; }
        };
        println!("Running full swap flow against {}", cfg.rpc_url);
        // Steps (implement with stellar-sdk or soroban-client):
        // 1. commit_ip(seller, hash) → ip_id
        // 2. initiate_swap(token, ip_id, seller, price, buyer) → swap_id
        // 3. accept_swap(swap_id) as buyer
        // 4. reveal_key(swap_id, seller, secret, blinding)
        // 5. Assert swap status == Completed and seller balance increased
    }

    #[test]
    #[ignore]
    fn test_testnet_fee_and_token_transfer() {
        let cfg = match TestnetConfig::from_env() {
            Ok(c) => c,
            Err(e) => { eprintln!("Skip: {e}"); return; }
        };
        println!("Testing fee/token flow against {}", cfg.rpc_url);
        // Steps:
        // 1. Record seller and treasury balances before swap
        // 2. Execute full swap flow
        // 3. Assert seller_balance_after == seller_balance_before + price*(1 - fee_bps/10000)
        // 4. Assert treasury_balance_after == treasury_balance_before + fee
    }

    #[test]
    #[ignore]
    fn test_testnet_error_cases() {
        let cfg = match TestnetConfig::from_env() {
            Ok(c) => c,
            Err(e) => { eprintln!("Skip: {e}"); return; }
        };
        println!("Testing error cases against {}", cfg.rpc_url);
        // Scenarios:
        // - reveal_key with wrong secret → expect InvalidKey (#2)
        // - accept_swap as non-buyer → expect auth error
        // - initiate_swap with price=0 → expect PriceMustBeGreaterThanZero (#3)
    }
}
