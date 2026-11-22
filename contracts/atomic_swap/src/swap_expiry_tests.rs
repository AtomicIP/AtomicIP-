// #1358: every swap initiation path must use `ProtocolConfig::swap_expiry_seconds`.
#[cfg(test)]
mod swap_expiry_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        vec, Address, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, DataKey, PaymentSchedule, ProtocolConfig};

    const START: u64 = 1_000_000;
    const SEVEN_DAYS: u64 = 604_800;
    const CUSTOM_EXPIRY: u64 = 3_600; // 1 hour, deliberately not the default
    const AUCTION_DURATION: u64 = 1_000;

    struct Ctx<'a> {
        env: &'a Env,
        client: AtomicSwapClient<'a>,
        registry_id: Address,
        token: Address,
        seller: Address,
        buyer: Address,
    }

    /// Deploys the contract and, when `expiry` is `Some`, stores a protocol
    /// config with that `swap_expiry_seconds` (the operator-set value).
    fn setup(env: &Env, expiry: Option<u64>) -> Ctx<'_> {
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = START);

        let admin = Address::generate(env);
        let seller = Address::generate(env);
        let buyer = Address::generate(env);

        let registry_id = env.register(IpRegistry, ());
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(&admin, &registry_id);

        if let Some(secs) = expiry {
            let config = ProtocolConfig {
                protocol_fee_bps: 250,
                treasury: Address::generate(env),
                dispute_window_seconds: 86_400,
                dispute_timeout_secs: SEVEN_DAYS,
                referral_fee_bps: 100,
                arbitration_timeout_seconds: 1_209_600,
                arbitration_ruling_delay_secs: 172_800,
                insurance_premium_bps: 200,
                swap_expiry_seconds: secs,
            };
            env.as_contract(&contract_id, || {
                env.storage()
                    .persistent()
                    .set(&DataKey::ProtocolConfig, &config);
            });
        }

        let token = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        Ctx {
            env,
            client,
            registry_id,
            token,
            seller,
            buyer,
        }
    }

    /// Commits a fresh IP owned by the seller.
    fn new_ip(ctx: &Ctx, seed: u8) -> u64 {
        IpRegistryClient::new(ctx.env, &ctx.registry_id)
            .commit_ip(&ctx.seller, &BytesN::from_array(ctx.env, &[seed; 32]), &0u32)
    }

    fn expiry_of(ctx: &Ctx, swap_id: u64) -> u64 {
        ctx.client.get_swap(&swap_id).expect("swap exists").expiry
    }

    #[test]
    fn test_default_expiry_is_seven_days_without_override() {
        let env = Env::default();
        let ctx = setup(&env, None);
        let ip = new_ip(&ctx, 0x01);

        let id = ctx.client.initiate_swap(
            &ctx.token,
            &ip,
            &ctx.seller,
            &1_000i128,
            &ctx.buyer,
            &0u32,
            &None,
            &0i128,
            &false,
        );

        assert_eq!(expiry_of(&ctx, id), START + SEVEN_DAYS);
    }

    #[test]
    fn test_initiate_swap_uses_configured_expiry() {
        let env = Env::default();
        let ctx = setup(&env, Some(CUSTOM_EXPIRY));
        let ip = new_ip(&ctx, 0x02);

        let id = ctx.client.initiate_swap(
            &ctx.token,
            &ip,
            &ctx.seller,
            &1_000i128,
            &ctx.buyer,
            &0u32,
            &None,
            &0i128,
            &false,
        );

        assert_eq!(expiry_of(&ctx, id), START + CUSTOM_EXPIRY);
    }

    #[test]
    fn test_schedule_path_uses_configured_expiry() {
        let env = Env::default();
        let ctx = setup(&env, Some(CUSTOM_EXPIRY));
        let ip = new_ip(&ctx, 0x03);
        let schedule = vec![
            &env,
            PaymentSchedule {
                due_timestamp: START + 100,
                amount: 1_000,
            },
        ];

        let id = ctx.client.initiate_swap_with_schedule(
            &ctx.token,
            &ip,
            &ctx.seller,
            &schedule,
            &ctx.buyer,
        );

        assert_eq!(expiry_of(&ctx, id), START + CUSTOM_EXPIRY);
    }

    #[test]
    fn test_installment_path_uses_configured_expiry() {
        let env = Env::default();
        let ctx = setup(&env, Some(CUSTOM_EXPIRY));
        let ip = new_ip(&ctx, 0x04);

        let id = ctx.client.initiate_swap_installment(
            &ctx.token,
            &ip,
            &ctx.seller,
            &1_000i128,
            &ctx.buyer,
            &2u32,
        );

        assert_eq!(expiry_of(&ctx, id), START + CUSTOM_EXPIRY);
    }

    #[test]
    fn test_batch_insured_path_uses_configured_expiry() {
        let env = Env::default();
        let ctx = setup(&env, Some(CUSTOM_EXPIRY));
        let ip = new_ip(&ctx, 0x05);

        let ids = ctx.client.batch_initiate_swap_insured(
            &ctx.token,
            &vec![&env, ip],
            &ctx.seller,
            &vec![&env, 1_000i128],
            &ctx.buyer,
            &0u32,
            &None,
            &false,
        );

        assert_eq!(expiry_of(&ctx, ids.get(0).unwrap()), START + CUSTOM_EXPIRY);
    }

    #[test]
    fn test_signers_path_uses_configured_expiry() {
        let env = Env::default();
        let ctx = setup(&env, Some(CUSTOM_EXPIRY));
        let ip = new_ip(&ctx, 0x06);
        let signer = Address::generate(&env);

        let id = ctx.client.initiate_swap_with_signers(
            &ctx.token,
            &ip,
            &ctx.seller,
            &1_000i128,
            &ctx.buyer,
            &vec![&env, signer],
        );

        assert_eq!(expiry_of(&ctx, id), START + CUSTOM_EXPIRY);
    }

    #[test]
    fn test_finalize_auction_path_uses_configured_expiry() {
        let env = Env::default();
        let ctx = setup(&env, Some(CUSTOM_EXPIRY));
        let ip = new_ip(&ctx, 0x07);

        let auction_id = ctx.client.start_ip_auction(
            &ctx.token,
            &ip,
            &ctx.seller,
            &1_000i128,
            &AUCTION_DURATION,
            &0u32,
            &0u64,
        );

        StellarAssetClient::new(&env, &ctx.token).mint(&ctx.buyer, &1_000);
        ctx.client.place_bid(&auction_id, &ctx.buyer, &1_000i128);

        let finalized_at = START + AUCTION_DURATION;
        env.ledger().with_mut(|l| l.timestamp = finalized_at);
        ctx.client.finalize_auction(&auction_id);

        // The swap created by finalization is the first swap (id 0).
        assert_eq!(expiry_of(&ctx, 0), finalized_at + CUSTOM_EXPIRY);
    }
}
