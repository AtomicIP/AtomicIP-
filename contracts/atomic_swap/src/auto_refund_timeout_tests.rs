#[cfg(test)]
mod auto_refund_timeout_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::{StellarAssetClient, TokenClient},
        Address, Bytes, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, DataKey, SwapRecord, SwapStatus};

    const PRICE: i128 = 1_000;
    const PAID: i128 = 300;
    const COLLATERAL: i128 = 50;
    const ARBITRATION_TS: u64 = 1_000;
    /// Default `arbitration_timeout_seconds` is 14 days; advance well past it.
    const AFTER_TIMEOUT: u64 = ARBITRATION_TS + 1_209_600 + 1;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn setup_registry(env: &Env, owner: &Address) -> (Address, u64) {
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);

        let secret = BytesN::from_array(env, &[0xAAu8; 32]);
        let blinding = BytesN::from_array(env, &[0xBBu8; 32]);

        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(secret.clone()));
        preimage.append(&Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

        let ip_id = registry.commit_ip(owner, &commitment_hash, &0u32);
        (registry_id, ip_id)
    }

    /// Registers a disputed swap directly in storage and funds the contract with what
    /// the buyer deposited (`paid` plus collateral). Returns the swap's token address.
    fn setup_disputed_swap(
        env: &Env,
        contract_id: &Address,
        ip_id: u64,
        seller: &Address,
        buyer: &Address,
        is_installment: bool,
        paid: i128,
        collateral: i128,
    ) -> Address {
        let token_id = env
            .register_stellar_asset_contract_v2(seller.clone())
            .address();
        let token_admin = StellarAssetClient::new(env, &token_id);
        token_admin.mint(contract_id, &(paid + collateral));

        let swap = SwapRecord {
            ip_id,
            seller: seller.clone(),
            buyer: buyer.clone(),
            price: PRICE,
            token: token_id.clone(),
            status: SwapStatus::Disputed,
            expiry: 9_999_999,
            accept_timestamp: ARBITRATION_TS,
            required_approvals: 0,
            dispute_timestamp: ARBITRATION_TS,
            referrer: None,
            collateral_amount: collateral,
            insurance_premium: 0,
            insurance_enabled: false,
            escrow_agent: None,
            quantity: 1,
            conditions: soroban_sdk::Vec::new(env),
            paid_amount: paid,
            is_installment,
            arbitrator: None,
        };

        env.as_contract(contract_id, || {
            env.storage().persistent().set(&DataKey::Swap(0u64), &swap);
            env.storage()
                .persistent()
                .set(&DataKey::ActiveSwap(ip_id), &0u64);
            env.storage()
                .persistent()
                .set(&DataKey::ArbitrationTimestamp(0u64), &ARBITRATION_TS);
            if collateral > 0 {
                env.storage()
                    .persistent()
                    .set(&DataKey::SwapCollateral(0u64), &collateral);
            }
        });

        token_id
    }

    fn setup_contract(env: &Env, registry_id: &Address) -> (Address, AtomicSwapClient) {
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(registry_id);
        (contract_id, client)
    }

    fn advance_past_timeout(env: &Env) {
        env.ledger().with_mut(|li| li.timestamp = AFTER_TIMEOUT);
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_installment_timeout_refunds_only_paid_amount() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry(&env, &seller);
        let (contract_id, client) = setup_contract(&env, &registry_id);
        let token_id = setup_disputed_swap(
            &env,
            &contract_id,
            ip_id,
            &seller,
            &buyer,
            true,
            PAID,
            0,
        );

        advance_past_timeout(&env);
        client.auto_refund_timeout(&0u64);

        let token = TokenClient::new(&env, &token_id);
        assert_eq!(token.balance(&buyer), PAID);
        assert_eq!(token.balance(&contract_id), 0);
    }

    #[test]
    fn test_collateral_is_returned_on_timeout() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry(&env, &seller);
        let (contract_id, client) = setup_contract(&env, &registry_id);
        let token_id = setup_disputed_swap(
            &env,
            &contract_id,
            ip_id,
            &seller,
            &buyer,
            false,
            PRICE,
            COLLATERAL,
        );

        advance_past_timeout(&env);
        client.auto_refund_timeout(&0u64);

        let token = TokenClient::new(&env, &token_id);
        assert_eq!(token.balance(&buyer), PRICE + COLLATERAL);
        assert_eq!(token.balance(&contract_id), 0);
        env.as_contract(&contract_id, || {
            assert!(!env
                .storage()
                .persistent()
                .has(&DataKey::SwapCollateral(0u64)));
        });
    }

    #[test]
    fn test_installment_timeout_refunds_paid_amount_and_collateral() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry(&env, &seller);
        let (contract_id, client) = setup_contract(&env, &registry_id);
        let token_id = setup_disputed_swap(
            &env,
            &contract_id,
            ip_id,
            &seller,
            &buyer,
            true,
            PAID,
            COLLATERAL,
        );

        advance_past_timeout(&env);
        client.auto_refund_timeout(&0u64);

        // Partial price plus collateral, never the full `price`.
        let token = TokenClient::new(&env, &token_id);
        assert_eq!(token.balance(&buyer), PAID + COLLATERAL);
        assert_eq!(token.balance(&contract_id), 0);
    }
}
