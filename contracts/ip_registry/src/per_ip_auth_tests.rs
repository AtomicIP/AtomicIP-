//! Authorization tests for per-IP writers (#1155).

#[cfg(test)]
mod per_ip_auth_tests {
    use crate::{IpRegistry, IpRegistryClient};
    use soroban_sdk::testutils::{Address as _, MockAuth, MockAuthInvoke};
    use soroban_sdk::{Address, Bytes, BytesN, Env, IntoVal, Vec};

    fn setup() -> (Env, IpRegistryClient<'static>, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(IpRegistry, ());
        let client = IpRegistryClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[0x77u8; 32]);
        client.commit_ip(&owner, &hash, &0u32);
        (env, client, owner, contract_id)
    }

    /// Authorize exactly one address for one call.
    fn only_authorize(
        env: &Env,
        contract_id: &Address,
        signer: &Address,
        fn_name: &'static str,
        args: soroban_sdk::Vec<soroban_sdk::Val>,
    ) {
        env.mock_auths(&[MockAuth {
            address: signer,
            invoke: &MockAuthInvoke {
                contract: contract_id,
                fn_name,
                args,
                sub_invokes: &[],
            },
        }]);
    }

    #[test]
    fn encrypt_commitment_by_non_owner_rejected() {
        let (env, client, _owner, contract_id) = setup();
        let attacker = Address::generate(&env);
        let encrypted = Bytes::from_array(&env, &[0xEEu8; 32]);
        let key_hint = BytesN::from_array(&env, &[0xABu8; 32]);

        only_authorize(
            &env,
            &contract_id,
            &attacker,
            "encrypt_commitment",
            soroban_sdk::vec![
                &env,
                1u64.into_val(&env),
                encrypted.clone().into_val(&env),
                key_hint.clone().into_val(&env),
            ],
        );
        assert!(client
            .try_encrypt_commitment(&1u64, &encrypted, &key_hint)
            .is_err());
        assert!(client.get_encrypted_commitment(&1u64).is_none());
    }

    #[test]
    fn encrypt_commitment_by_owner_accepted() {
        let (env, client, owner, contract_id) = setup();
        let encrypted = Bytes::from_array(&env, &[0xCDu8; 32]);
        let key_hint = BytesN::from_array(&env, &[0x0Fu8; 32]);

        only_authorize(
            &env,
            &contract_id,
            &owner,
            "encrypt_commitment",
            soroban_sdk::vec![
                &env,
                1u64.into_val(&env),
                encrypted.clone().into_val(&env),
                key_hint.clone().into_val(&env),
            ],
        );
        client.encrypt_commitment(&1u64, &encrypted, &key_hint);
        assert_eq!(
            client.get_encrypted_commitment(&1u64).unwrap().encrypted_hash,
            encrypted
        );
    }

    #[test]
    fn threshold_config_by_non_owner_rejected() {
        let (env, client, _owner, contract_id) = setup();
        let attacker = Address::generate(&env);
        let signers: Vec<Address> = soroban_sdk::vec![&env, Address::generate(&env)];

        only_authorize(
            &env,
            &contract_id,
            &attacker,
            "require_threshold_signatures",
            soroban_sdk::vec![
                &env,
                1u64.into_val(&env),
                1u32.into_val(&env),
                signers.clone().into_val(&env),
            ],
        );
        assert!(client
            .try_require_threshold_signatures(&1u64, &1u32, &signers)
            .is_err());
        assert!(client.get_threshold_config(&1u64).is_none());
    }

    #[test]
    fn threshold_signature_must_be_authorized_by_signer() {
        let (env, client, owner, contract_id) = setup();
        let signer = Address::generate(&env);
        let signers: Vec<Address> = soroban_sdk::vec![&env, signer.clone()];
        client.require_threshold_signatures(&1u64, &1u32, &signers);
        let sig = BytesN::from_array(&env, &[0x5Au8; 32]);

        // The owner cannot sign on behalf of the signer.
        only_authorize(
            &env,
            &contract_id,
            &owner,
            "add_threshold_signature",
            soroban_sdk::vec![
                &env,
                1u64.into_val(&env),
                signer.clone().into_val(&env),
                sig.clone().into_val(&env),
            ],
        );
        assert!(client
            .try_add_threshold_signature(&1u64, &signer, &sig)
            .is_err());

        // The signer authorizes their own signature.
        only_authorize(
            &env,
            &contract_id,
            &signer,
            "add_threshold_signature",
            soroban_sdk::vec![
                &env,
                1u64.into_val(&env),
                signer.clone().into_val(&env),
                sig.clone().into_val(&env),
            ],
        );
        client.add_threshold_signature(&1u64, &signer, &sig);
    }
}
