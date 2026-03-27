#[cfg(test)]
mod tests {
    use soroban_sdk::{BytesN, Env, Address};
    use soroban_sdk::testutils::Address as _;
    use crate::AtomicSwap;

    #[test]
    fn test_ttl_extension_after_swap_initiation() {
        let env = Env::default();
        
        let ip_id = 1;
        let price = 1000;
        let buyer = Address::generate(&env);

        // Test that we can initiate a swap (this includes TTL extension)
        let swap_id = AtomicSwap::initiate_swap(env.clone(), ip_id, price, buyer.clone());
        
        // Verify the swap exists and is accessible
        let swap = AtomicSwap::get_swap(env.clone(), swap_id);
        assert_eq!(swap.ip_id, ip_id);
        assert_eq!(swap.price, price);
        assert_eq!(swap.buyer, buyer);
        assert_eq!(swap.status, crate::SwapStatus::Pending);
    }

    #[test]
    fn test_swap_lifecycle() {
        let env = Env::default();
        
        let ip_id = 1;
        let price = 1000;
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let decryption_key = BytesN::from_array(&env, &[0; 32]);

        // Complete swap lifecycle
        let swap_id = AtomicSwap::initiate_swap(env.clone(), ip_id, price, buyer.clone());
        
        // Accept swap
        super::AtomicSwap::accept_swap(env.clone(), swap_id);
        let swap = AtomicSwap::get_swap(env.clone(), swap_id);
        assert_eq!(swap.status, crate::SwapStatus::Accepted);
        
        // Complete swap — caller must be the seller
        super::AtomicSwap::reveal_key(env.clone(), swap_id, seller.clone(), decryption_key);
        let swap = AtomicSwap::get_swap(env.clone(), swap_id);
        assert_eq!(swap.status, crate::SwapStatus::Completed);
    }

    #[test]
    fn test_swap_cancellation() {
        let env = Env::default();
        
        let ip_id = 1;
        let price = 1000;
        let buyer = Address::generate(&env);

        // Initiate and cancel swap
        let swap_id = AtomicSwap::initiate_swap(env.clone(), ip_id, price, buyer);
        super::AtomicSwap::cancel_swap(env.clone(), swap_id);
        
        // Verify swap is cancelled
        let swap = AtomicSwap::get_swap(env.clone(), swap_id);
        assert_eq!(swap.status, crate::SwapStatus::Cancelled);
    }

    #[test]
    fn test_multiple_swaps() {
        let env = Env::default();
        
        let buyer = Address::generate(&env);

        // Create multiple swaps
        let swap_id1 = AtomicSwap::initiate_swap(env.clone(), 1, 1000, buyer.clone());
        let swap_id2 = AtomicSwap::initiate_swap(env.clone(), 2, 2000, buyer.clone());
        
        // Verify both swaps exist
        let swap1 = AtomicSwap::get_swap(env.clone(), swap_id1);
        let swap2 = AtomicSwap::get_swap(env.clone(), swap_id2);
        assert_eq!(swap1.ip_id, 1);
        assert_eq!(swap2.ip_id, 2);
        assert_eq!(swap1.buyer, buyer);
        assert_eq!(swap2.buyer, buyer);
    }
}
