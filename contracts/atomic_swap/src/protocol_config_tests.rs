/// Regression tests for protocol configuration (#1329, #1330).
#[cfg(test)]
mod protocol_config_tests {
    use ip_registry::IpRegistry;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    use crate::{AtomicSwap, AtomicSwapClient, ContractError};

    fn setup(env: &Env) -> AtomicSwapClient<'_> {
        env.mock_all_auths();
        let registry_id = env.register(IpRegistry, ());
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        let treasury = Address::generate(env);
        client.initialize(&registry_id, &treasury);
        client
    }

    /// The all-zero contract address that `require_valid_treasury_address` rejects.
    fn zero_treasury(env: &Env) -> Address {
        Address::from_str(
            env,
            "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4",
        )
    }

    /// #1329: `admin_set_protocol_config` must reject the zero/placeholder treasury.
    #[test]
    fn test_admin_set_protocol_config_rejects_zero_treasury() {
        let env = Env::default();
        let client = setup(&env);

        let err = client
            .try_admin_set_protocol_config(
                &250u32,
                &zero_treasury(&env),
                &86_400u64,
                &604_800u64,
                &100u32,
                &200u32,
            )
            .unwrap_err()
            .unwrap();

        assert_eq!(
            err,
            soroban_sdk::Error::from_contract_error(ContractError::InvalidTreasuryAddress as u32)
        );
    }

    /// #1329: a valid, non-placeholder treasury is still accepted.
    #[test]
    fn test_admin_set_protocol_config_accepts_valid_treasury() {
        let env = Env::default();
        let client = setup(&env);
        let treasury = Address::generate(&env);

        client.admin_set_protocol_config(&250u32, &treasury, &86_400u64, &604_800u64, &100u32, &200u32);
    }
}
