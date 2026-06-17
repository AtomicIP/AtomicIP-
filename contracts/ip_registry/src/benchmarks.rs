/// #551 Performance Benchmarking Suite — IP Registry
///
/// Measures CPU instruction budget consumed by each core operation.
/// Soroban's instruction budget is deterministic for a given SDK version,
/// making these tests reliable regression guards.
///
/// Run with: cargo test bench_ -p ip_registry
#[cfg(test)]
mod benchmarks {
    use soroban_sdk::{
        testutils::{Address as _, budget::Budget},
        Address, Bytes, BytesN, Env,
    };

    use crate::{IpRegistry, IpRegistryClient};

    // CPU instruction limits (conservative upper bounds).
    const COMMIT_IP_CPU_LIMIT: u64 = 500_000;
    const VERIFY_COMMITMENT_CPU_LIMIT: u64 = 200_000;
    const GET_IP_CPU_LIMIT: u64 = 100_000;
    const LIST_IP_BY_OWNER_CPU_LIMIT: u64 = 150_000;

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

    #[test]
    fn bench_commit_ip() {
        let (env, client) = setup();
        let owner = Address::generate(&env);
        let secret = BytesN::from_array(&env, &[0x01u8; 32]);
        let blinding = BytesN::from_array(&env, &[0x02u8; 32]);
        let hash = make_commitment(&env, &secret, &blinding);

        env.budget().reset_default();
        client.commit_ip(&owner, &hash);
        let cpu = env.budget().cpu_instruction_count();

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
        let ip_id = client.commit_ip(&owner, &hash);

        env.budget().reset_default();
        client.verify_commitment(&ip_id, &secret, &blinding);
        let cpu = env.budget().cpu_instruction_count();

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
        let ip_id = client.commit_ip(&owner, &hash);

        env.budget().reset_default();
        client.get_ip(&ip_id);
        let cpu = env.budget().cpu_instruction_count();

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
            client.commit_ip(&owner, &hash);
        }

        env.budget().reset_default();
        client.list_ip_by_owner(&owner);
        let cpu = env.budget().cpu_instruction_count();

        assert!(
            cpu <= LIST_IP_BY_OWNER_CPU_LIMIT,
            "bench_list_ip_by_owner: {} instructions exceeds limit of {}",
            cpu,
            LIST_IP_BY_OWNER_CPU_LIMIT
        );
    }
}
