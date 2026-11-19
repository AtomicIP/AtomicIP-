#[cfg(test)]
mod ip_swaps_index_tests {
    use soroban_sdk::{Env, Vec};

    use crate::{append_ip_swap, AtomicSwap, AtomicSwapClient, DataKey};

    fn setup_contract(env: &Env) -> (soroban_sdk::Address, AtomicSwapClient) {
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        (contract_id, client)
    }

    #[test]
    fn test_index_returns_ids_in_creation_order_for_1000_swaps() {
        let env = Env::default();
        let (contract_id, client) = setup_contract(&env);
        const IP_ID: u64 = 7;

        env.as_contract(&contract_id, || {
            for swap_id in 0..1_000u64 {
                append_ip_swap(&env, IP_ID, swap_id);
            }
        });

        let ids = client.get_swaps_by_ip(&IP_ID).unwrap();
        assert_eq!(ids.len(), 1_000);
        assert_eq!(ids.get(0).unwrap(), 0);
        assert_eq!(ids.get(999).unwrap(), 999);
    }

    #[test]
    fn test_page_returns_requested_window() {
        let env = Env::default();
        let (contract_id, client) = setup_contract(&env);
        const IP_ID: u64 = 7;

        env.as_contract(&contract_id, || {
            for swap_id in 0..1_000u64 {
                append_ip_swap(&env, IP_ID, swap_id);
            }
        });

        let tail = client.get_swaps_by_ip_page(&IP_ID, &990u64, &20u32);
        assert_eq!(tail.len(), 10);
        assert_eq!(tail.get(0).unwrap(), 990);
        assert_eq!(tail.get(9).unwrap(), 999);

        let head = client.get_swaps_by_ip_page(&IP_ID, &0u64, &5u32);
        assert_eq!(head.len(), 5);
        assert_eq!(head.get(0).unwrap(), 0);
        assert_eq!(head.get(4).unwrap(), 4);

        let past_end = client.get_swaps_by_ip_page(&IP_ID, &1_000u64, &10u32);
        assert_eq!(past_end.len(), 0);
    }

    #[test]
    fn test_page_limit_is_capped_at_max_batch_size() {
        let env = Env::default();
        let (contract_id, client) = setup_contract(&env);
        const IP_ID: u64 = 7;

        env.as_contract(&contract_id, || {
            for swap_id in 0..200u64 {
                append_ip_swap(&env, IP_ID, swap_id);
            }
        });

        let page = client.get_swaps_by_ip_page(&IP_ID, &0u64, &u32::MAX);
        assert_eq!(page.len(), crate::MAX_BATCH_SIZE);
    }

    #[test]
    fn test_ip_without_swaps_returns_none_and_empty_page() {
        let env = Env::default();
        let (_contract_id, client) = setup_contract(&env);

        assert!(client.get_swaps_by_ip(&42u64).is_none());
        assert_eq!(client.get_swaps_by_ip_page(&42u64, &0u64, &10u32).len(), 0);
    }

    #[test]
    fn test_legacy_vector_is_readable_and_migrated_on_append() {
        let env = Env::default();
        let (contract_id, client) = setup_contract(&env);
        const IP_ID: u64 = 9;

        env.as_contract(&contract_id, || {
            let mut legacy: Vec<u64> = Vec::new(&env);
            legacy.push_back(100);
            legacy.push_back(101);
            env.storage()
                .persistent()
                .set(&DataKey::IpSwaps(IP_ID), &legacy);
        });

        // Before migration, reads still see the legacy vector.
        let before = client.get_swaps_by_ip_page(&IP_ID, &0u64, &1u32);
        assert_eq!(before.len(), 1);
        assert_eq!(before.get(0).unwrap(), 100);

        env.as_contract(&contract_id, || {
            append_ip_swap(&env, IP_ID, 102);
            assert!(!env.storage().persistent().has(&DataKey::IpSwaps(IP_ID)));
        });

        let ids = client.get_swaps_by_ip(&IP_ID).unwrap();
        assert_eq!(ids.len(), 3);
        assert_eq!(ids.get(0).unwrap(), 100);
        assert_eq!(ids.get(1).unwrap(), 101);
        assert_eq!(ids.get(2).unwrap(), 102);
    }
}
