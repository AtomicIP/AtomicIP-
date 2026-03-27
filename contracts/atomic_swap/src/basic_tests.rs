#[cfg(test)]
mod tests {
    use soroban_sdk::{BytesN, Env, Address};
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_basic_functionality() {
        let env = Env::default();
        
        // Test that we can create basic types and the environment works
        let buyer = Address::generate(&env);
        let decryption_key = BytesN::from_array(&env, &[0; 32]);
        
        // Verify basic functionality
        assert_eq!(decryption_key.len(), 32);
        
        // Test that we can create multiple addresses
        let buyer2 = Address::generate(&env);
        assert_ne!(buyer, buyer2);
    }

    #[test]
    fn test_storage_keys() {
        let env = Env::default();
        
        // Test that our storage keys work correctly
        let key = crate::DataKey::Swap(1);
        let key2 = crate::DataKey::Swap(2);
        assert_ne!(key, key2);
        
        let next_id_key = crate::DataKey::NextId;
        assert_ne!(key, next_id_key);
    }

    #[test]
    fn test_swap_record_creation() {
        let env = Env::default();
        
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let price = 1000;
        let ip_id = 1;
        
        // Test that we can create SwapRecord struct
        let token = Address::generate(&env);
        let swap = crate::SwapRecord {
            ip_id,
            seller: seller.clone(),
            buyer: buyer.clone(),
            price,
            token,
            expiry: 0,
            status: crate::SwapStatus::Pending,
        };
        
        assert_eq!(swap.seller, seller);
        assert_eq!(swap.buyer, buyer);
        assert_eq!(swap.price, price);
        assert_eq!(swap.status, crate::SwapStatus::Pending);
    }

    #[test]
    fn test_swap_status_enum() {
        // Test that all enum variants work
        let status1 = crate::SwapStatus::Pending;
        let status2 = crate::SwapStatus::Accepted;
        let status3 = crate::SwapStatus::Completed;
        let status4 = crate::SwapStatus::Cancelled;
        
        assert_ne!(status1, status2);
        assert_ne!(status2, status3);
        assert_ne!(status3, status4);
        assert_ne!(status4, status1);
    }
}
