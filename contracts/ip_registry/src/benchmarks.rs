/// #551 / #817 Performance Benchmarking Suite — IP Registry
///
/// Measures CPU instruction budget consumed by each core operation.
/// Soroban's instruction budget is deterministic for a given SDK version,
/// making these tests reliable regression guards.
///
/// #817 adds a dedicated benchmark for `batch_verify_commitments` (the ZK
/// Pedersen+Schnorr proof path) so that instruction counts are on record for
/// fee estimation and ledger-limit planning.  Typical and worst-case (batch of
/// 10) costs are captured.  Results are documented in docs/commitment-scheme.md.
///
/// Run with: cargo test bench_ -p ip_registry
#[cfg(test)]
mod benchmarks {
    use soroban_sdk::{
        testutils::Address as _,
        Address, Bytes, BytesN, Env, Vec,
    };

    use crate::{
        zk_commitment::test_prover, HidingCommitmentProof, HidingVerifyRequest,
        IpRegistry, IpRegistryClient,
    };

    // ── CPU instruction limits (conservative upper bounds) ────────────────────

    const COMMIT_IP_CPU_LIMIT: u64 = 600_000;
    const VERIFY_COMMITMENT_CPU_LIMIT: u64 = 200_000;
    const GET_IP_CPU_LIMIT: u64 = 100_000;
    const LIST_IP_BY_OWNER_CPU_LIMIT: u64 = 150_000;

    /// #817: Budget for a single ZK proof verification via `batch_verify_commitments`.
    /// Pedersen commitment + Schnorr verification involves ~3 scalar mults on
    /// Ristretto255, which is dominated by the curve arithmetic cost.
    const ZK_VERIFY_SINGLE_CPU_LIMIT: u64 = 5_000_000;

    /// #817: Budget for a batch of 10 ZK proof verifications.
    const ZK_VERIFY_BATCH10_CPU_LIMIT: u64 = 50_000_000;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_sha256_commitment(env: &Env, secret: &BytesN<32>, blinding: &BytesN<32>) -> BytesN<32> {
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

    // ── #551 existing benchmarks (fixed) ──────────────────────────────────────

    #[test]
    fn bench_commit_ip() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let secret = BytesN::from_array(&env, &[0x01u8; 32]);
        let blinding = BytesN::from_array(&env, &[0x02u8; 32]);
        let hash = make_sha256_commitment(&env, &secret, &blinding);

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
        let hash = make_sha256_commitment(&env, &secret, &blinding);
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
        let hash = make_sha256_commitment(&env, &secret, &blinding);
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
            let hash = make_sha256_commitment(&env, &secret, &blinding);
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

    // ── #817: ZK commitment proof verification benchmarks ────────────────────

    /// Build a Pedersen commitment and a valid hiding proof using fixed test nonces.
    ///
    /// `index` is used to vary the secret/blinding values across calls so each
    /// IP has a unique commitment hash.
    fn make_zk_commitment_and_proof(
        env: &Env,
        index: u8,
    ) -> (BytesN<32>, HidingCommitmentProof) {
        let secret = BytesN::from_array(env, &[index; 32]);
        let blinding = BytesN::from_array(env, &[index.wrapping_add(0x40); 32]);
        // Nonces must be non-zero scalars for a valid proof; use fixed distinct values.
        let nonce_s = BytesN::from_array(env, &[index.wrapping_add(0x10); 32]);
        let nonce_b = BytesN::from_array(env, &[index.wrapping_add(0x20); 32]);

        let commitment = test_prover::pedersen_commit(env, &secret, &blinding);
        let proof = test_prover::prove_hiding(env, &secret, &blinding, &commitment, &nonce_s, &nonce_b);
        (commitment, proof)
    }

    /// #817: Benchmark a single ZK hiding proof verification.
    ///
    /// This exercises `batch_verify_commitments` with exactly one entry,
    /// isolating the per-proof cost (curve decompression + scalar mults + equality check).
    ///
    /// Recorded instruction count is the *typical* cost baseline documented in
    /// docs/commitment-scheme.md.
    #[test]
    fn bench_zk_verify_single_proof() {
        let (env, client) = setup();
        let owner = Address::generate(&env);

        // Register an IP using the Pedersen commitment hash.
        let (commitment, proof) = make_zk_commitment_and_proof(&env, 0x01);
        let ip_id = client.commit_ip(&owner, &commitment, &0u32);

        // Build the batch request with a single entry.
        let mut requests: Vec<HidingVerifyRequest> = Vec::new(&env);
        requests.push_back(HidingVerifyRequest {
            ip_id,
            proof,
        });

        env.cost_estimate().budget().reset_default();
        let results = client.batch_verify_commitments(&requests);
        let cpu = env.cost_estimate().budget().cpu_instruction_cost();

        // The single proof must verify successfully.
        assert_eq!(results.len(), 1);
        assert!(results.get(0).unwrap().valid, "ZK proof must verify for bench_zk_verify_single_proof");

        assert!(
            cpu <= ZK_VERIFY_SINGLE_CPU_LIMIT,
            "bench_zk_verify_single_proof: {} instructions exceeds limit of {}",
            cpu,
            ZK_VERIFY_SINGLE_CPU_LIMIT
        );
    }

    /// #817: Benchmark a batch of 10 ZK hiding proof verifications (worst-case).
    ///
    /// Captures the *worst-case* cost for a realistic batch size. The cost
    /// grows linearly with batch size; this bound is documented in
    /// docs/commitment-scheme.md alongside the per-proof baseline.
    #[test]
    fn bench_zk_verify_batch_10_proofs() {
        let (env, client) = setup();
        let owner = Address::generate(&env);

        // Register 10 IPs with distinct Pedersen commitments.
        let mut requests: Vec<HidingVerifyRequest> = Vec::new(&env);
        for i in 1u8..=10 {
            let (commitment, proof) = make_zk_commitment_and_proof(&env, i);
            let ip_id = client.commit_ip(&owner, &commitment, &0u32);
            requests.push_back(HidingVerifyRequest { ip_id, proof });
        }

        env.cost_estimate().budget().reset_default();
        let results = client.batch_verify_commitments(&requests);
        let cpu = env.cost_estimate().budget().cpu_instruction_cost();

        // All 10 proofs must verify successfully.
        assert_eq!(results.len(), 10);
        for r in results.iter() {
            assert!(r.valid, "all 10 ZK proofs must verify in batch");
        }

        assert!(
            cpu <= ZK_VERIFY_BATCH10_CPU_LIMIT,
            "bench_zk_verify_batch_10_proofs: {} instructions exceeds limit of {}",
            cpu,
            ZK_VERIFY_BATCH10_CPU_LIMIT
        );
    }
}
