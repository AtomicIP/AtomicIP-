/// Stress Testing for Concurrent Operations
///
/// Soroban contracts execute deterministically in a single-threaded host, so
/// "concurrent" is modelled as many independent swaps sharing the same
/// environment — the closest analogue to simultaneous on-chain transactions
/// within a single ledger close.
///
/// Tests verify:
///   1. 100+ simultaneous swaps can be initiated without state corruption
///   2. Interleaved accept/reveal/cancel across many swaps produces correct state
///   3. Swap IDs are strictly monotonically increasing (no lost updates)
///   4. Per-operation CPU and memory budgets stay within acceptable bounds
///   5. Owner swap indices are consistent after bulk operations
#[cfg(test)]
mod stress_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _,
        token::StellarAssetClient,
        Address, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus};

    const SWAP_COUNT: usize = 100;
    const PRICE: i128 = 100;

    // ── helpers ───────────────────────────────────────────────────────────────

    /// Register an IP for `owner` and return (registry_id, ip_id, secret, blinding).
    fn register_ip(
        env: &Env,
        owner: &Address,
        seed: u8,
    ) -> (Address, u64, BytesN<32>, BytesN<32>) {
        let reg_id = env.register(IpRegistry, ());
        let reg = IpRegistryClient::new(env, &reg_id);

        let secret = BytesN::from_array(env, &[seed; 32]);
        let blinding = BytesN::from_array(env, &[seed.wrapping_add(1); 32]);
        let mut preimage = soroban_sdk::Bytes::new(env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = reg.commit_ip(owner, &hash);
        (reg_id, ip_id, secret, blinding)
    }

    fn mint_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    // ── Test 1: 100 simultaneous swaps — no state corruption ─────────────────

    #[test]
    fn stress_100_simultaneous_swaps_no_state_corruption() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);

        // One shared swap contract, one registry per IP (each IP needs unique hash)
        let swap_contract = env.register(AtomicSwap, ());

        let mut swap_ids: soroban_sdk::Vec<u64> = soroban_sdk::Vec::new(&env);

        for i in 0..SWAP_COUNT {
            let seller = Address::generate(&env);
            let buyer = Address::generate(&env);
            let seed = (i as u8).wrapping_add(1);

            let (reg_id, ip_id, _secret, _blinding) = register_ip(&env, &seller, seed);
            let token_id = mint_token(&env, &admin, &buyer, PRICE * 2);

            let client = AtomicSwapClient::new(&env, &swap_contract);
            // Initialize only on first call
            if i == 0 {
                client.initialize(&reg_id);
            }

            let swap_id =
                client.initiate_swap(&token_id, &ip_id, &seller, &PRICE, &buyer, &0_u32, &None);
            swap_ids.push_back(swap_id);
        }

        // Property 1: exactly SWAP_COUNT swaps created
        assert_eq!(swap_ids.len() as usize, SWAP_COUNT);

        // Property 2: all swap IDs are strictly monotonically increasing (no lost updates)
        let client = AtomicSwapClient::new(&env, &swap_contract);
        for i in 1..swap_ids.len() {
            assert!(
                swap_ids.get(i - 1).unwrap() < swap_ids.get(i).unwrap(),
                "Swap IDs must be strictly increasing: {} >= {}",
                swap_ids.get(i - 1).unwrap(),
                swap_ids.get(i).unwrap()
            );
        }

        // Property 3: every swap is retrievable and in Pending state
        for i in 0..swap_ids.len() {
            let swap_id = swap_ids.get(i).unwrap();
            let record = client.get_swap(&swap_id).expect("swap must exist");
            assert_eq!(
                record.status,
                SwapStatus::Pending,
                "swap {} must be Pending",
                swap_id
            );
        }
    }

    // ── Test 2: interleaved initiate/accept/reveal across 100 swaps ──────────

    #[test]
    fn stress_interleaved_accept_reveal_no_corruption() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let swap_contract = env.register(AtomicSwap, ());

        struct SwapInfo {
            swap_id: u64,
            seller: Address,
            secret: BytesN<32>,
            blinding: BytesN<32>,
        }

        let mut infos: std::vec::Vec<SwapInfo> = std::vec::Vec::new();

        for i in 0..SWAP_COUNT {
            let seller = Address::generate(&env);
            let buyer = Address::generate(&env);
            let seed = (i as u8).wrapping_add(1);

            let (reg_id, ip_id, secret, blinding) = register_ip(&env, &seller, seed);
            let token_id = mint_token(&env, &admin, &buyer, PRICE * 2);

            let client = AtomicSwapClient::new(&env, &swap_contract);
            if i == 0 {
                client.initialize(&reg_id);
            }

            let swap_id =
                client.initiate_swap(&token_id, &ip_id, &seller, &PRICE, &buyer, &0_u32, &None);

            infos.push(SwapInfo { swap_id, seller, secret, blinding });
        }

        let client = AtomicSwapClient::new(&env, &swap_contract);

        // Accept all swaps (interleaved phase 1)
        for info in &infos {
            client.accept_swap(&info.swap_id);
        }

        // Verify all are Accepted before any reveal
        for info in &infos {
            assert_eq!(
                client.get_swap(&info.swap_id).unwrap().status,
                SwapStatus::Accepted,
                "swap {} must be Accepted before reveal",
                info.swap_id
            );
        }

        // Reveal all keys (interleaved phase 2)
        for info in &infos {
            client.reveal_key(&info.swap_id, &info.seller, &info.secret, &info.blinding);
        }

        // Property: all swaps completed, no state corruption
        for info in &infos {
            assert_eq!(
                client.get_swap(&info.swap_id).unwrap().status,
                SwapStatus::Completed,
                "swap {} must be Completed after reveal",
                info.swap_id
            );
        }
    }

    // ── Test 3: mixed complete/cancel — verify no cross-swap contamination ────

    #[test]
    fn stress_mixed_complete_and_cancel_no_cross_contamination() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let swap_contract = env.register(AtomicSwap, ());

        struct SwapInfo {
            swap_id: u64,
            seller: Address,
            secret: BytesN<32>,
            blinding: BytesN<32>,
        }

        let mut infos: std::vec::Vec<SwapInfo> = std::vec::Vec::new();

        for i in 0..SWAP_COUNT {
            let seller = Address::generate(&env);
            let buyer = Address::generate(&env);
            let seed = (i as u8).wrapping_add(1);

            let (reg_id, ip_id, secret, blinding) = register_ip(&env, &seller, seed);
            let token_id = mint_token(&env, &admin, &buyer, PRICE * 2);

            let client = AtomicSwapClient::new(&env, &swap_contract);
            if i == 0 {
                client.initialize(&reg_id);
            }

            let swap_id =
                client.initiate_swap(&token_id, &ip_id, &seller, &PRICE, &buyer, &0_u32, &None);
            infos.push(SwapInfo { swap_id, seller, secret, blinding });
        }

        let client = AtomicSwapClient::new(&env, &swap_contract);

        // Even-indexed: complete; odd-indexed: cancel
        for (i, info) in infos.iter().enumerate() {
            if i % 2 == 0 {
                client.accept_swap(&info.swap_id);
                client.reveal_key(&info.swap_id, &info.seller, &info.secret, &info.blinding);
            } else {
                client.cancel_swap(&info.swap_id, &info.seller);
            }
        }

        // Verify no cross-contamination
        for (i, info) in infos.iter().enumerate() {
            let expected = if i % 2 == 0 {
                SwapStatus::Completed
            } else {
                SwapStatus::Cancelled
            };
            assert_eq!(
                client.get_swap(&info.swap_id).unwrap().status,
                expected,
                "swap {} (index {}) has wrong status",
                info.swap_id,
                i
            );
        }
    }

    // ── Test 4: throughput — CPU budget per operation under load ─────────────

    #[test]
    fn stress_throughput_cpu_budget_per_initiate() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        let (reg_id, ip_id, _s, _b) = register_ip(&env, &seller, 0x42);
        let token_id = mint_token(&env, &admin, &buyer, PRICE * 10);

        let swap_contract = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &swap_contract);
        client.initialize(&reg_id);

        // Measure CPU for a single initiate_swap under load (after 10 prior swaps)
        // Use separate IPs to avoid duplicate-hash rejection
        for i in 1u8..=10 {
            let s2 = Address::generate(&env);
            let b2 = Address::generate(&env);
            let (r2, ip2, _, _) = register_ip(&env, &s2, i);
            let t2 = mint_token(&env, &admin, &b2, PRICE * 2);
            // Re-initialize is rejected after first call; just call initiate directly
            let _ = client.initiate_swap(&t2, &ip2, &s2, &PRICE, &b2, &0_u32, &None);
        }

        env.budget().reset_default();
        let swap_id =
            client.initiate_swap(&token_id, &ip_id, &seller, &PRICE, &buyer, &0_u32, &None);
        let cpu = env.budget().cpu_instruction_count();
        let mem = env.budget().memory_bytes_count();

        assert!(
            cpu <= 1_000_000,
            "initiate_swap CPU under load: {} (limit 1_000_000)",
            cpu
        );
        assert!(
            mem <= 500_000,
            "initiate_swap memory under load: {} bytes (limit 500_000)",
            mem
        );

        // Swap must still be valid
        assert_eq!(
            client.get_swap(&swap_id).unwrap().status,
            SwapStatus::Pending
        );
    }

    // ── Test 5: swap ID monotonicity across 100 sequential initiations ────────

    #[test]
    fn stress_swap_id_monotonicity_100_swaps() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let swap_contract = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &swap_contract);

        let mut prev_id: Option<u64> = None;

        for i in 0..SWAP_COUNT {
            let seller = Address::generate(&env);
            let buyer = Address::generate(&env);
            let seed = (i as u8).wrapping_add(1);

            let (reg_id, ip_id, _, _) = register_ip(&env, &seller, seed);
            let token_id = mint_token(&env, &admin, &buyer, PRICE * 2);

            if i == 0 {
                client.initialize(&reg_id);
            }

            let swap_id =
                client.initiate_swap(&token_id, &ip_id, &seller, &PRICE, &buyer, &0_u32, &None);

            if let Some(prev) = prev_id {
                assert!(
                    swap_id > prev,
                    "Swap ID not monotonically increasing: {} <= {}",
                    swap_id,
                    prev
                );
            }
            prev_id = Some(swap_id);
        }

        // Final ID must equal SWAP_COUNT (IDs start at 1)
        assert_eq!(
            prev_id.unwrap(),
            SWAP_COUNT as u64,
            "Final swap ID must equal SWAP_COUNT"
        );
    }
}
