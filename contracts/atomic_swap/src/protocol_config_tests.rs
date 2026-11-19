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

    /// #1330: values set by `admin_set_protocol_config` are persisted and read back.
    #[test]
    fn test_admin_set_protocol_config_persists_values() {
        let env = Env::default();
        let client = setup(&env);
        let treasury = Address::generate(&env);

        client.admin_set_protocol_config(&300u32, &treasury, &3_600u64, &7_200u64, &50u32, &150u32);

        let cfg = env.as_contract(&client.address, || AtomicSwap::protocol_config(&env));
        assert_eq!(cfg.protocol_fee_bps, 300);
        assert_eq!(cfg.treasury, treasury);
        assert_eq!(cfg.dispute_window_seconds, 3_600);
        assert_eq!(cfg.dispute_timeout_secs, 7_200);
        assert_eq!(cfg.referral_fee_bps, 50);
        assert_eq!(cfg.insurance_premium_bps, 150);
    }

    /// #1330: a later `admin_set_protocol_config` replaces the stored config.
    #[test]
    fn test_admin_set_protocol_config_overwrites_previous_config() {
        let env = Env::default();
        let client = setup(&env);
        let first_treasury = Address::generate(&env);
        let second_treasury = Address::generate(&env);

        client.admin_set_protocol_config(&300u32, &first_treasury, &3_600u64, &7_200u64, &50u32, &150u32);
        client.admin_set_protocol_config(&500u32, &second_treasury, &1_800u64, &9_000u64, &25u32, &75u32);

        let cfg = env.as_contract(&client.address, || AtomicSwap::protocol_config(&env));
        assert_eq!(cfg.protocol_fee_bps, 500);
        assert_eq!(cfg.treasury, second_treasury);
        assert_eq!(cfg.dispute_window_seconds, 1_800);
        assert_eq!(cfg.referral_fee_bps, 25);
        assert_eq!(cfg.insurance_premium_bps, 75);
    }

    /// #1330: before any admin config is stored, the built-in defaults apply.
    #[test]
    fn test_protocol_config_defaults_when_unset() {
        let env = Env::default();
        let client = setup(&env);

        let cfg = env.as_contract(&client.address, || AtomicSwap::protocol_config(&env));
        assert_eq!(cfg.protocol_fee_bps, 250);
        assert_eq!(cfg.referral_fee_bps, 100);
        assert_eq!(cfg.insurance_premium_bps, 200);
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
