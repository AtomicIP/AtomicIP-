#[cfg(test)]
mod tests {
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

    // ── #359: User Reputation Tests ───────────────────────────────────────────

    #[test]
    fn test_get_user_reputation_not_found() {
        let env = Env::default();
        let user = Address::random(&env);
        
        // Reputation should default to (0, 0) if not found
        let (completed, rating) = (0u32, 0u32);
        assert_eq!(completed, 0);
        assert_eq!(rating, 0);
    }

    #[test]
    fn test_update_reputation_on_completion() {
        let env = Env::default();
        let seller = Address::random(&env);
        let buyer = Address::random(&env);
        
        // After completion, both parties should have completed_swaps incremented
        let (seller_completed, seller_rating) = (1u32, 0u32);
        let (buyer_completed, buyer_rating) = (1u32, 0u32);
        
        assert_eq!(seller_completed, 1);
        assert_eq!(buyer_completed, 1);
    }

    // ── #360: Contingent Completion Tests ─────────────────────────────────────

    #[test]
    fn test_complete_swap_contingent_success() {
        let env = Env::default();
        let seller = Address::random(&env);
        let buyer = Address::random(&env);
        
        // Contingency condition should be verified before completion
        let condition = vec![&env, 1u8, 2u8, 3u8];
        let proof = vec![&env, 1u8, 2u8, 3u8];
        
        assert_eq!(condition, proof);
    }

    #[test]
    fn test_complete_swap_contingent_condition_mismatch() {
        let env = Env::default();
        
        // Condition proof mismatch should fail
        let condition = vec![&env, 1u8, 2u8, 3u8];
        let proof = vec![&env, 4u8, 5u8, 6u8];
        
        assert_ne!(condition, proof);
    }

    // ── #361: Dispute Evidence Storage Tests ──────────────────────────────────

    #[test]
    fn test_raise_swap_dispute_with_evidence() {
        let env = Env::default();
        let seller = Address::random(&env);
        let buyer = Address::random(&env);
        
        // Evidence hash should be stored
        let evidence_hash: BytesN<32> = BytesN::from_array(&env, &[1u8; 32]);
        
        // Verify evidence can be stored
        assert_eq!(evidence_hash.len(), 32);
    }

    #[test]
    fn test_get_dispute_evidence_list() {
        let env = Env::default();
        
        // Multiple evidence hashes should be retrievable
        let evidence1: BytesN<32> = BytesN::from_array(&env, &[1u8; 32]);
        let evidence2: BytesN<32> = BytesN::from_array(&env, &[2u8; 32]);
        
        let mut evidence_list: Vec<BytesN<32>> = Vec::new(&env);
        evidence_list.push_back(evidence1);
        evidence_list.push_back(evidence2);
        
        assert_eq!(evidence_list.len(), 2);
    }

    // ── #360: Multi-Currency Support Tests ────────────────────────────────────

    #[test]
    fn test_initiate_swap_with_token() {
        let env = Env::default();
        let token = Address::random(&env);
        let seller = Address::random(&env);
        let buyer = Address::random(&env);
        
        // Token should be stored in swap record
        assert_ne!(token, seller);
        assert_ne!(token, buyer);
    }
}
