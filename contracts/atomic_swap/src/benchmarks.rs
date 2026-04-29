/// #377 Performance Benchmarking Suite — Atomic Swap
///
/// Measures CPU instruction budget for:
///   - initiate_swap
///   - reveal_key
///
/// Regression targets:
///   - initiate_swap: ≤ 800_000 CPU instructions
///   - reveal_key:    ≤ 600_000 CPU instructions
#[cfg(test)]
mod benchmarks {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _,
        token::StellarAssetClient,
        Address, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient};

    // ── helpers ──────────────────────────────────────────────────────────────

    fn setup(env: &Env) -> (AtomicSwapClient, Address, Address, Address, u64, BytesN<32>, BytesN<32>) {
        env.mock_all_auths();

        let seller = Address::generate(env);
        let buyer = Address::generate(env);
        let admin = Address::generate(env);

        // IP registry
        let reg_id = env.register(IpRegistry, ());
        let reg = IpRegistryClient::new(env, &reg_id);

        let secret = BytesN::from_array(env, &[0x11u8; 32]);
        let blinding = BytesN::from_array(env, &[0x22u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = reg.commit_ip(&seller, &commitment_hash);

        // Token
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(env, &token_id).mint(&buyer, &10_000_i128);

        // Swap contract
        let swap_id = env.register(AtomicSwap, ());
        let swap = AtomicSwapClient::new(env, &swap_id);
        swap.initialize(&reg_id);

        (swap, token_id, seller, buyer, ip_id, secret, blinding)
    }

    // ── initiate_swap ────────────────────────────────────────────────────────

    #[test]
    fn bench_initiate_swap_cpu_budget() {
        let env = Env::default();
        let (swap, token_id, seller, buyer, ip_id, _secret, _blinding) = setup(&env);

        env.budget().reset_default();
        swap.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0u32, &None, &false);
        let cpu = env.budget().cpu_instruction_count();

        assert!(
            cpu <= 800_000,
            "initiate_swap CPU regression: {} instructions (limit 800_000)",
            cpu
        );
    }

    #[test]
    fn bench_initiate_swap_mem_budget() {
        let env = Env::default();
        let (swap, token_id, seller, buyer, ip_id, _secret, _blinding) = setup(&env);

        env.budget().reset_default();
        swap.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0u32, &None, &false);
        let mem = env.budget().memory_bytes_count();

        assert!(
            mem <= 300_000,
            "initiate_swap memory regression: {} bytes (limit 300_000)",
            mem
        );
    }

    // ── reveal_key ───────────────────────────────────────────────────────────

    #[test]
    fn bench_reveal_key_cpu_budget() {
        let env = Env::default();
        let (swap, token_id, seller, buyer, ip_id, secret, blinding) = setup(&env);

        let swap_id = swap.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0u32, &None, &false);
        swap.accept_swap(&swap_id);

        env.budget().reset_default();
        swap.reveal_key(&swap_id, &seller, &secret, &blinding);
        let cpu = env.budget().cpu_instruction_count();

        assert!(
            cpu <= 600_000,
            "reveal_key CPU regression: {} instructions (limit 600_000)",
            cpu
        );
    }

    #[test]
    fn bench_reveal_key_mem_budget() {
        let env = Env::default();
        let (swap, token_id, seller, buyer, ip_id, secret, blinding) = setup(&env);

        let swap_id = swap.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0u32, &None, &false);
        swap.accept_swap(&swap_id);

        env.budget().reset_default();
        swap.reveal_key(&swap_id, &seller, &secret, &blinding);
        let mem = env.budget().memory_bytes_count();

        assert!(
            mem <= 300_000,
            "reveal_key memory regression: {} bytes (limit 300_000)",
            mem
        );
    }
}
