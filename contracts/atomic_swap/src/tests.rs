#[cfg(test)]
mod tests {
    use soroban_sdk::{BytesN, Env, Address};
    use crate::AtomicSwapClient;

    use super::{AtomicSwap, DataKey, SwapRecord, SwapStatus};

    #[test]
    fn test_ttl_extension_after_swap_initiation() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AtomicSwap);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let ip_id = 1;
        let price = 1000;
        let buyer = Address::random(&env);

        // Initiate swap
        let swap_id = client.initiate_swap(&ip_id, &price, &buyer);

        // Verify the swap exists
        let swap = client.get_swap(&swap_id);
        assert_eq!(swap.ip_id, ip_id);
        assert_eq!(swap.status, SwapStatus::Pending);

        // Check that TTL was extended
        let ttl = env.storage().persistent().get_ttl(&DataKey::Swap(swap_id)).unwrap();
        assert!(ttl > 0, "TTL should be extended after swap initiation");

        // Simulate ledger progression
        env.jump(1000);

        // Swap should still be accessible
        let swap_after = client.get_swap(&swap_id);
        assert_eq!(swap_after.status, SwapStatus::Pending);
    }

    #[test]
    fn test_ttl_extension_after_swap_acceptance() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AtomicSwap);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let ip_id = 1;
        let price = 1000;
        let buyer = Address::random(&env);

        // Initiate and accept swap
        let swap_id = client.initiate_swap(&ip_id, &price, &buyer);
        client.accept_swap(&swap_id);

        // Check TTL after acceptance
        let ttl = env.storage().persistent().get_ttl(&DataKey::Swap(swap_id)).unwrap();
        assert!(ttl > 0, "TTL should be extended after swap acceptance");

        // Simulate ledger progression
        env.jump(1000);

        // Swap should still be accessible with updated status
        let swap = client.get_swap(&swap_id);
        assert_eq!(swap.status, SwapStatus::Accepted);
    }

    #[test]
    fn test_ttl_extension_after_swap_completion() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AtomicSwap);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let ip_id = 1;
        let price = 1000;
        let buyer = Address::random(&env);
        let seller = Address::random(&env);
        let decryption_key = BytesN::from_array(&env, &[0; 32]);

        // Complete swap lifecycle
        let swap_id = client.initiate_swap(&ip_id, &price, &buyer);
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &decryption_key);

        // Check TTL after completion
        let ttl = env.storage().persistent().get_ttl(&DataKey::Swap(swap_id)).unwrap();
        assert!(ttl > 0, "TTL should be extended after swap completion");

        // Simulate ledger progression
        env.jump(1000);

        // Swap should still be accessible with completed status
        let swap = client.get_swap(&swap_id);
        assert_eq!(swap.status, SwapStatus::Completed);
    }

    #[test]
    fn test_ttl_extension_after_swap_cancellation() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AtomicSwap);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let ip_id = 1;
        let price = 1000;
        let buyer = Address::random(&env);

        // Initiate and cancel swap
        let swap_id = client.initiate_swap(&ip_id, &price, &buyer);
        client.cancel_swap(&swap_id);

        // Check TTL after cancellation
        let ttl = env.storage().persistent().get_ttl(&DataKey::Swap(swap_id)).unwrap();
        assert!(ttl > 0, "TTL should be extended after swap cancellation");

        // Simulate ledger progression
        env.jump(1000);

        // Swap should still be accessible with cancelled status
        let swap = client.get_swap(&swap_id);
        assert_eq!(swap.status, SwapStatus::Cancelled);
    }

    #[test]
    fn test_multiple_ttl_extensions_during_swap_lifecycle() {
        let env = Env::default();
        let contract_id = env.register_contract(None, AtomicSwap);
        let client = AtomicSwapClient::new(&env, &contract_id);

        let ip_id = 1;
        let price = 1000;
        let seller = Address::random(&env);
        let buyer = Address::random(&env);
        let decryption_key = BytesN::from_array(&env, &[0; 32]);

        // Track TTL through entire lifecycle
        let swap_id = client.initiate_swap(&ip_id, &price, &buyer);
        let ttl_after_init = env.storage().persistent().get_ttl(&DataKey::Swap(swap_id)).unwrap();

        env.jump(100);
        client.accept_swap(&swap_id);
        let ttl_after_accept = env.storage().persistent().get_ttl(&DataKey::Swap(swap_id)).unwrap();

        env.jump(100);
        client.reveal_key(&swap_id, &seller, &decryption_key);
        let ttl_after_complete = env.storage().persistent().get_ttl(&DataKey::Swap(swap_id)).unwrap();

        // All TTLs should be positive
        assert!(ttl_after_init > 0);
        assert!(ttl_after_accept > 0);
        assert!(ttl_after_complete > 0);

        // Final state should be preserved
        env.jump(1000);
        let final_swap = client.get_swap(&swap_id);
        assert_eq!(final_swap.status, SwapStatus::Completed);
    }
}
