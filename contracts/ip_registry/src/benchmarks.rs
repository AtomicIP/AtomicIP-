/// #551 / #635 Performance Benchmarking Suite — IP Registry
///
/// Measures CPU instruction budget consumed by each core operation.
/// Soroban's instruction budget is deterministic for a given SDK version,
/// making these tests reliable regression guards.
///
/// Run with: cargo test bench_ -p ip_registry -- --nocapture
#[cfg(test)]
mod benchmarks {
    use soroban_sdk::{
        contractclient, testutils::Address as _, Address, Bytes, BytesN, Env, IntoVal, Vec,
    };

    use crate::{IpRecord, IpRegistry, VerifyRequest, VerifyResult};

    #[contractclient(name = "IpRegistryClient")]
    #[allow(dead_code)]
    trait IpRegistryTrait {
        fn commit_ip(env: Env, owner: Address, commitment_hash: BytesN<32>, pow_difficulty: u32) -> u64;
        fn batch_commit_ip(env: Env, owner: Address, commitment_hashes: Vec<BytesN<32>>) -> Vec<u64>;
        fn get_ip(env: Env, ip_id: u64) -> IpRecord;
        fn verify_commitment(env: Env, ip_id: u64, secret: BytesN<32>, blinding_factor: BytesN<32>) -> bool;
        fn list_ip_by_owner(env: Env, owner: Address) -> Vec<u64>;
        fn batch_verify_commitments(env: Env, requests: Vec<VerifyRequest>) -> Vec<VerifyResult>;
        fn is_ip_owner(env: Env, ip_id: u64, address: Address) -> bool;
        fn transfer_ip(env: Env, ip_id: u64, new_owner: Address);
    }

    // CPU instruction limits (conservative upper bounds).
    const COMMIT_IP_CPU_LIMIT: u64 = 600_000;
    const VERIFY_COMMITMENT_CPU_LIMIT: u64 = 600_000;
    const GET_IP_CPU_LIMIT: u64 = 100_000;
    const LIST_IP_BY_OWNER_CPU_LIMIT: u64 = 150_000;
    const BATCH_COMMIT_CPU_LIMIT: u64 = 30_000_000;
    const BATCH_VERIFY_CPU_LIMIT: u64 = 1_200_000;

    // #635: SLA baselines (p99 latency targets in ms at Soroban level).
    // These are instruction-budget-based approximations.
    pub const SLA_COMMIT_IP_P99_MS: u64 = 300;
    pub const SLA_VERIFY_COMMITMENT_P99_MS: u64 = 100;
    pub const SLA_GET_IP_P99_MS: u64 = 50;

    fn make_commitment(env: &Env, secret: &BytesN<32>, blinding: &BytesN<32>) -> BytesN<32> {
        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from(secret.clone()));
        preimage.append(&Bytes::from(blinding.clone()));
        env.crypto().sha256(&preimage).into()
    }

    fn setup() -> (Env, IpRegistryClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &id);
        (env, client)
    }

    // ── Unit Benchmarks (single-operation instruction budget) ─────────────────

    #[test]
    fn bench_commit_ip() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let secret = BytesN::from_array(&env, &[0x01u8; 32]);
        let blinding = BytesN::from_array(&env, &[0x02u8; 32]);
        let hash = make_commitment(&env, &secret, &blinding);

        env.cost_estimate().budget().reset_default();
        client.commit_ip(&owner, &hash, &0u32);
        let cpu = env.cost_estimate().budget().cpu_instruction_cost();

        assert!(
            cpu <= COMMIT_IP_CPU_LIMIT,
            "bench_commit_ip: {} instructions exceeds limit of {}",
            cpu,
            COMMIT_IP_CPU_LIMIT
        );
    }

    #[test]
    fn bench_verify_commitment() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let secret = BytesN::from_array(&env, &[0x03u8; 32]);
        let blinding = BytesN::from_array(&env, &[0x04u8; 32]);
        let hash = make_commitment(&env, &secret, &blinding);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        env.cost_estimate().budget().reset_default();
        client.verify_commitment(&ip_id, &secret, &blinding);
        let cpu = env.cost_estimate().budget().cpu_instruction_cost();

        assert!(
            cpu <= VERIFY_COMMITMENT_CPU_LIMIT,
            "bench_verify_commitment: {} instructions exceeds limit of {}",
            cpu,
            VERIFY_COMMITMENT_CPU_LIMIT
        );
    }

    #[test]
    fn bench_get_ip() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let secret = BytesN::from_array(&env, &[0x05u8; 32]);
        let blinding = BytesN::from_array(&env, &[0x06u8; 32]);
        let hash = make_commitment(&env, &secret, &blinding);
        let ip_id = client.commit_ip(&owner, &hash, &0u32);

        env.cost_estimate().budget().reset_default();
        client.get_ip(&ip_id);
        let cpu = env.cost_estimate().budget().cpu_instruction_cost();

        assert!(
            cpu <= GET_IP_CPU_LIMIT,
            "bench_get_ip: {} instructions exceeds limit of {}",
            cpu,
            GET_IP_CPU_LIMIT
        );
    }

    #[test]
    fn bench_list_ip_by_owner() {
        let (env, client) = setup();
        let owner = Address::generate(&env);

        // Pre-populate 5 IPs.
        for i in 1u8..=5 {
            let secret = BytesN::from_array(&env, &[i; 32]);
            let blinding = BytesN::from_array(&env, &[i.wrapping_add(0x80); 32]);
            let hash = make_commitment(&env, &secret, &blinding);
            client.commit_ip(&owner, &hash, &0u32);
        }

        env.cost_estimate().budget().reset_default();
        client.list_ip_by_owner(&owner);
        let cpu = env.cost_estimate().budget().cpu_instruction_cost();

        assert!(
            cpu <= LIST_IP_BY_OWNER_CPU_LIMIT,
            "bench_list_ip_by_owner: {} instructions exceeds limit of {}",
            cpu,
            LIST_IP_BY_OWNER_CPU_LIMIT
        );
    }

    // ── #635: Load Testing Scenarios ──────────────────────────────────────────

    /// Simulate 1000+ concurrent IP commit operations.
    /// Measures total CPU instructions and asserts SLA compliance.
    #[test]
    fn bench_load_1000_commits() {
        let (env, client) = setup();
        let owner = Address::generate(&env);

        env.cost_estimate().budget().reset_default();
        let mut total_cpu: u64 = 0;

        for i in 1..=1000 {
            let mut hash_bytes = [0u8; 32];
            hash_bytes[..4].copy_from_slice(&(i as u32).to_be_bytes());
            let hash = BytesN::from_array(&env, &hash_bytes);
            client.commit_ip(&owner, &hash, &0u32);
            total_cpu += env.cost_estimate().budget().cpu_instruction_cost();
            env.cost_estimate().budget().reset_default();
        }

        let avg_cpu = total_cpu / 1000;
        let total_ids = client.list_ip_by_owner(&owner);
        assert_eq!(total_ids.len(), 1000);
        // Average per-commit CPU should be under the limit
        assert!(
            avg_cpu <= COMMIT_IP_CPU_LIMIT,
            "bench_load_1000_commits: avg {} instructions exceeds limit of {}",
            avg_cpu,
            COMMIT_IP_CPU_LIMIT
        );
    }

    /// Simulate concurrent batch commits (simulating 100 users each committing 10 IPs).
    #[test]
    fn bench_load_batch_commits_concurrent() {
        let (env, client) = setup();

        env.cost_estimate().budget().reset_default();
        let mut total_cpu: u64 = 0;

        // Simulate 100 users each committing 10 IPs in a batch.
        let mut global_idx: u32 = 1;
        for _user_id in 0..100 {
            let owner = Address::generate(&env);
            let mut hashes = Vec::new(&env);
            for _j in 0..10 {
                let mut hash_bytes = [0u8; 32];
                hash_bytes[..4].copy_from_slice(&global_idx.to_be_bytes());
                hashes.push_back(BytesN::from_array(&env, &hash_bytes));
                global_idx += 1;
            }
            client.batch_commit_ip(&owner, &hashes);
            total_cpu += env.cost_estimate().budget().cpu_instruction_cost();
            env.cost_estimate().budget().reset_default();
        }

        let avg_cpu = total_cpu / 100;
        assert!(
            avg_cpu <= BATCH_COMMIT_CPU_LIMIT,
            "bench_load_batch_commits_concurrent: avg {} instructions exceeds limit of {}",
            avg_cpu,
            BATCH_COMMIT_CPU_LIMIT
        );
    }

    /// Simulate sustained verification traffic (1000 verify_commitment calls).
    #[test]
    fn bench_load_1000_verifications() {
        let (env, client) = setup();
        let owner = Address::generate(&env);

        // Pre-populate 100 IPs with known secrets.
        let mut secrets = Vec::new(&env);
        let mut blindings = Vec::new(&env);
        let mut ip_ids = Vec::new(&env);

        for i in 0..100u8 {
            let secret = BytesN::from_array(&env, &[i; 32]);
            let blinding = BytesN::from_array(&env, &[i.wrapping_add(0x80); 32]);
            let hash = make_commitment(&env, &secret, &blinding);
            let ip_id = client.commit_ip(&owner, &hash, &0u32);
            secrets.push_back(secret);
            blindings.push_back(blinding);
            ip_ids.push_back(ip_id);
        }

        env.cost_estimate().budget().reset_default();
        let mut total_cpu: u64 = 0;

        // Run 1000 verifications cycling through the 100 IPs.
        for i in 0..1000 {
            let idx = (i % 100) as u32;
            let secret = secrets.get(idx).unwrap();
            let blinding = blindings.get(idx).unwrap();
            let ip_id = ip_ids.get(idx).unwrap();
            client.verify_commitment(&ip_id, &secret, &blinding);
            total_cpu += env.cost_estimate().budget().cpu_instruction_cost();
            env.cost_estimate().budget().reset_default();
        }

        let avg_cpu = total_cpu / 1000;
        assert!(
            avg_cpu <= VERIFY_COMMITMENT_CPU_LIMIT,
            "bench_load_1000_verifications: avg {} instructions exceeds limit of {}",
            avg_cpu,
            VERIFY_COMMITMENT_CPU_LIMIT
        );
    }

    /// Simulate batch verification under load (100 batch requests with 10 items each).
    #[test]
    fn bench_load_batch_verify() {
        let (env, client) = setup();
        let owner = Address::generate(&env);

        // Commit 100 IPs.
        let mut secrets = Vec::new(&env);
        let mut blindings = Vec::new(&env);
        let mut ip_ids = Vec::new(&env);

        for i in 0..100u8 {
            let secret = BytesN::from_array(&env, &[i; 32]);
            let blinding = BytesN::from_array(&env, &[i.wrapping_add(0x80); 32]);
            let hash = make_commitment(&env, &secret, &blinding);
            let ip_id = client.commit_ip(&owner, &hash, &0u32);
            secrets.push_back(secret);
            blindings.push_back(blinding);
            ip_ids.push_back(ip_id);
        }

        env.cost_estimate().budget().reset_default();
        let mut total_cpu: u64 = 0;

        // 100 batch verification requests, each with 10 items.
        for batch in 0..100 {
            let mut requests = Vec::new(&env);
            for j in 0..10 {
                let idx = ((batch * 10 + j) % 100) as u32;
                requests.push_back(VerifyRequest {
                    ip_id: ip_ids.get(idx).unwrap(),
                    secret: secrets.get(idx).unwrap(),
                    blinding_factor: blindings.get(idx).unwrap(),
                });
            }
            client.batch_verify_commitments(&requests);
            total_cpu += env.cost_estimate().budget().cpu_instruction_cost();
            env.cost_estimate().budget().reset_default();
        }

        let avg_cpu = total_cpu / 100;
        assert!(
            avg_cpu <= BATCH_VERIFY_CPU_LIMIT,
            "bench_load_batch_verify: avg {} instructions exceeds limit of {}",
            avg_cpu,
            BATCH_VERIFY_CPU_LIMIT
        );
    }

    /// #635: SLA compliance — verify that core operations fall within p99 latency budgets.
    #[test]
    fn bench_sla_compliance() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let secret = BytesN::from_array(&env, &[0x01u8; 32]);
        let blinding = BytesN::from_array(&env, &[0x02u8; 32]);
        let hash = make_commitment(&env, &secret, &blinding);

        // Measure commit_ip latency
        env.cost_estimate().budget().reset_default();
        let ip_id = client.commit_ip(&owner, &hash, &0u32);
        let commit_cpu = env.cost_estimate().budget().cpu_instruction_cost();

        // Measure verify_commitment latency
        env.cost_estimate().budget().reset_default();
        client.verify_commitment(&ip_id, &secret, &blinding);
        let verify_cpu = env.cost_estimate().budget().cpu_instruction_cost();

        // Measure get_ip latency
        env.cost_estimate().budget().reset_default();
        client.get_ip(&ip_id);
        let get_cpu = env.cost_estimate().budget().cpu_instruction_cost();

        // Assert SLA baselines (instruction budget as proxy for p99 latency)
        assert!(
            commit_cpu <= COMMIT_IP_CPU_LIMIT,
            "SLA breach: commit_ip {} instructions > {} limit",
            commit_cpu,
            COMMIT_IP_CPU_LIMIT
        );
        assert!(
            verify_cpu <= VERIFY_COMMITMENT_CPU_LIMIT,
            "SLA breach: verify_commitment {} instructions > {} limit",
            verify_cpu,
            VERIFY_COMMITMENT_CPU_LIMIT
        );
        assert!(
            get_cpu <= GET_IP_CPU_LIMIT,
            "SLA breach: get_ip {} instructions > {} limit",
            get_cpu,
            GET_IP_CPU_LIMIT
        );
    }
}
