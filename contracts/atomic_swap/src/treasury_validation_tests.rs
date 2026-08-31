/// Test suite for treasury address validation
/// Issue #906: Guard against hardcoded placeholder treasury addresses
/// This module ensures that zero and well-known placeholder addresses
/// cannot be set as the treasury during contract initialization.

#[cfg(test)]
mod tests {
    use soroban_sdk::{Address, Env};
    use crate::validation::require_valid_treasury_address;
    use crate::ContractError;

    #[test]
    fn test_require_valid_treasury_rejects_zero_address() {
        let env = Env::default();
        let zero_address = Address::from_contract_id(&env, &soroban_sdk::BytesN::<32>::new());

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            require_valid_treasury_address(&env, &zero_address);
        }));

        assert!(result.is_err(), "Should reject zero address");
    }

    #[test]
    fn test_require_valid_treasury_accepts_valid_address() {
        let env = Env::default();
        let valid_treasury = Address::generate(&env);

        require_valid_treasury_address(&env, &valid_treasury);
    }

    #[test]
    fn test_require_valid_treasury_different_valid_addresses() {
        let env = Env::default();

        for _ in 0..5 {
            let valid_address = Address::generate(&env);
            require_valid_treasury_address(&env, &valid_address);
        }
    }
}
