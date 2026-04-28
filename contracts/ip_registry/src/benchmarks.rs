/// #377 Performance Benchmarking Suite — IP Registry
///
/// Measures execution time (CPU instructions via Soroban budget) for:
///   - commit_ip
///   - verify_commitment
///
/// Regression targets (conservative upper bounds):
///   - commit_ip:          ≤ 500_000 CPU instructions
///   - verify_commitment:  ≤ 200_000 CPU instructions
#[cfg(test)]
mod benchmarks {
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

    use crate::{IpRegistry, IpRegistryClient};

    fn make_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env.budget().reset_default();
        env
    }

    fn register(env: &Env) -> IpRegistryClient {
        let id = env.register(IpRegistry, ());
        IpRegistryClient::new(env, &id)
    }

    fn unique_hash(env: &Env, seed: u8) -> BytesN<32> {
        BytesN::from_array(env, &[seed; 32])
    }

    // ── commit_ip ────────────────────────────────────────────────────────────

    #[test]
    fn bench_commit_ip_cpu_budget() {
        let env = make_env();
        let client = register(&env);
        let owner = Address::generate(&env);
        let hash = unique_hash(&env, 0xAB);

        env.budget().reset_default();
        client.commit_ip(&owner, &hash, &0u32);
        let cpu = env.budget().cpu_instruction_count();

        // Regression gate: must stay under 500_000 CPU instructions.
        assert!(
            cpu <= 500_000,
            "commit_ip CPU regression: {} instructions (limit 500_000)",
            cpu
        );
    }

    #[test]
    fn bench_commit_ip_mem_budget() {
        let env = make_env();
        let client = register(&env);
        let owner = Address::generate(&env);
        let hash = unique_hash(&env, 0xCD);

        env.budget().reset_default();
        client.commit_ip(&owner, &hash, &0u32);
        let mem = env.budget().memory_bytes_count();

        // Regression gate: must stay under 200_000 bytes.
        assert!(
            mem <= 200_000,
            "commit_ip memory regression: {} bytes (limit 200_000)",
            mem
        );
    }

    // ── verify_commitment ────────────────────────────────────────────────────

    #[test]
    fn bench_verify_commitment_cpu_budget() {
        let env = make_env();
        let client = register(&env);
        let owner = Address::generate(&env);

        let secret = BytesN::from_array(&env, &[0x01u8; 32]);
        let blinding = BytesN::from_array(&env, &[0x02u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

        client.commit_ip(&owner, &commitment_hash, &0u32);

        env.budget().reset_default();
        client.verify_commitment(&1u64, &secret, &blinding);
        let cpu = env.budget().cpu_instruction_count();

        assert!(
            cpu <= 200_000,
            "verify_commitment CPU regression: {} instructions (limit 200_000)",
            cpu
        );
    }

    // ── batch scaling ────────────────────────────────────────────────────────

    #[test]
    fn bench_commit_ip_scales_linearly() {
        let env = make_env();
        let client = register(&env);
        let owner = Address::generate(&env);

        // Warm up (first call initialises Admin key — slightly more expensive)
        client.commit_ip(&owner, &unique_hash(&env, 0x00), &0u32);

        env.budget().reset_default();
        client.commit_ip(&owner, &unique_hash(&env, 0x01), &0u32);
        let single = env.budget().cpu_instruction_count();

        env.budget().reset_default();
        for seed in 0x02u8..0x07u8 {
            client.commit_ip(&owner, &unique_hash(&env, seed), &0u32);
        }
        let five = env.budget().cpu_instruction_count();

        // Five calls should cost less than 6× a single call (linear + small overhead).
        assert!(
            five <= single * 6,
            "commit_ip scaling non-linear: 5× cost {} vs 6× single {}",
            five,
            single * 6
        );
    }
}
