/// #551 Performance Benchmarking Suite — Atomic Swap
///
/// Measures CPU instruction budget consumed by each core swap operation.
///
/// Run with: cargo test bench_ -p atomic_swap
#[cfg(test)]
mod benchmarks {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, budget::Budget},
        token::StellarAssetClient,
        Address, Bytes, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient};

    // CPU instruction limits (conservative upper bounds).
    const INITIATE_SWAP_CPU_LIMIT: u64 = 800_000;
    const ACCEPT_SWAP_CPU_LIMIT: u64 = 600_000;
    const REVEAL_KEY_CPU_LIMIT: u64 = 600_000;
    const CANCEL_SWAP_CPU_LIMIT: u64 = 400_000;
    const GET_SWAP_CPU_LIMIT: u64 = 100_000;

    fn make_commitment(env: &Env, secret: &BytesN<32>, blinding: &BytesN<32>) -> BytesN<32> {
        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(secret.clone()));
        preimage.append(&Bytes::from(blinding.clone()));
        env.crypto().sha256(&preimage).into()
    }

    struct BenchCtx {
        env: Env,
        swap: AtomicSwapClient<'static>,
        token: Address,
        registry_id: Address,
    }

    fn setup() -> BenchCtx {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let registry_id = env.register(IpRegistry, ());
        let token = env.register_stellar_asset_contract_v2(admin).address();
        let swap_id = env.register(AtomicSwap, ());
        let swap = AtomicSwapClient::new(&env, &swap_id);
        swap.initialize(&registry_id);
        BenchCtx { env, swap, token, registry_id }
    }

    fn commit_ip(ctx: &BenchCtx, seller: &Address, seed: u8) -> (u64, BytesN<32>, BytesN<32>) {
        let registry = IpRegistryClient::new(&ctx.env, &ctx.registry_id);
        let secret = BytesN::from_array(&ctx.env, &[seed; 32]);
        let blinding = BytesN::from_array(&ctx.env, &[seed.wrapping_add(0x80); 32]);
        let hash = make_commitment(&ctx.env, &secret, &blinding);
        let ip_id = registry.commit_ip(seller, &hash, &0u32);
        (ip_id, secret, blinding)
    }

    #[test]
    fn bench_initiate_swap() {
        let ctx = setup();
        let seller = Address::generate(&ctx.env);
        let buyer = Address::generate(&ctx.env);
        let (ip_id, _secret, _blinding) = commit_ip(&ctx, &seller, 0x01);
        StellarAssetClient::new(&ctx.env, &ctx.token).mint(&buyer, &1000);

        ctx.env.budget().reset_default();
        ctx.swap.initiate_swap(&ctx.token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false);
        let cpu = ctx.env.budget().cpu_instruction_count();

        assert!(
            cpu <= INITIATE_SWAP_CPU_LIMIT,
            "bench_initiate_swap: {} instructions exceeds limit of {}",
            cpu,
            INITIATE_SWAP_CPU_LIMIT
        );
    }

    #[test]
    fn bench_accept_swap() {
        let ctx = setup();
        let seller = Address::generate(&ctx.env);
        let buyer = Address::generate(&ctx.env);
        let (ip_id, _secret, _blinding) = commit_ip(&ctx, &seller, 0x02);
        StellarAssetClient::new(&ctx.env, &ctx.token).mint(&buyer, &1000);
        let swap_id = ctx.swap.initiate_swap(
            &ctx.token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );

        ctx.env.budget().reset_default();
        ctx.swap.accept_swap(&swap_id);
        let cpu = ctx.env.budget().cpu_instruction_count();

        assert!(
            cpu <= ACCEPT_SWAP_CPU_LIMIT,
            "bench_accept_swap: {} instructions exceeds limit of {}",
            cpu,
            ACCEPT_SWAP_CPU_LIMIT
        );
    }

    #[test]
    fn bench_reveal_key() {
        let ctx = setup();
        let seller = Address::generate(&ctx.env);
        let buyer = Address::generate(&ctx.env);
        let (ip_id, secret, blinding) = commit_ip(&ctx, &seller, 0x03);
        StellarAssetClient::new(&ctx.env, &ctx.token).mint(&buyer, &1000);
        let swap_id = ctx.swap.initiate_swap(
            &ctx.token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );
        ctx.swap.accept_swap(&swap_id);

        ctx.env.budget().reset_default();
        ctx.swap.reveal_key(&swap_id, &seller, &secret, &blinding);
        let cpu = ctx.env.budget().cpu_instruction_count();

        assert!(
            cpu <= REVEAL_KEY_CPU_LIMIT,
            "bench_reveal_key: {} instructions exceeds limit of {}",
            cpu,
            REVEAL_KEY_CPU_LIMIT
        );
    }

    #[test]
    fn bench_cancel_swap() {
        let ctx = setup();
        let seller = Address::generate(&ctx.env);
        let buyer = Address::generate(&ctx.env);
        let (ip_id, _secret, _blinding) = commit_ip(&ctx, &seller, 0x04);
        StellarAssetClient::new(&ctx.env, &ctx.token).mint(&buyer, &1000);
        let swap_id = ctx.swap.initiate_swap(
            &ctx.token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );

        ctx.env.budget().reset_default();
        ctx.swap.cancel_swap(&swap_id, &seller);
        let cpu = ctx.env.budget().cpu_instruction_count();

        assert!(
            cpu <= CANCEL_SWAP_CPU_LIMIT,
            "bench_cancel_swap: {} instructions exceeds limit of {}",
            cpu,
            CANCEL_SWAP_CPU_LIMIT
        );
    }

    #[test]
    fn bench_get_swap() {
        let ctx = setup();
        let seller = Address::generate(&ctx.env);
        let buyer = Address::generate(&ctx.env);
        let (ip_id, _secret, _blinding) = commit_ip(&ctx, &seller, 0x05);
        StellarAssetClient::new(&ctx.env, &ctx.token).mint(&buyer, &1000);
        let swap_id = ctx.swap.initiate_swap(
            &ctx.token, &ip_id, &seller, &1000, &buyer, &0_u32, &None, &0i128, &false,
        );

        ctx.env.budget().reset_default();
        ctx.swap.get_swap(&swap_id);
        let cpu = ctx.env.budget().cpu_instruction_count();

        assert!(
            cpu <= GET_SWAP_CPU_LIMIT,
            "bench_get_swap: {} instructions exceeds limit of {}",
            cpu,
            GET_SWAP_CPU_LIMIT
        );
    }
}
