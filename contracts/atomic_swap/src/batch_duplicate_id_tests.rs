/// #1341: property tests asserting that every batch entry point rejects a batch
/// containing a repeated ID with `DuplicateSwapId`. The check runs before any
/// state is read or written, so a rejected batch cannot change contract state.
#[cfg(test)]
mod batch_duplicate_id_tests {
    use proptest::prelude::*;
    use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, Vec};

    use crate::{AtomicSwap, AtomicSwapClient, ContractError};

    /// Builds a batch from `base` and appends a copy of one of its existing IDs,
    /// so the result always contains a duplicate.
    fn batch_with_duplicate(env: &Env, base: &[u64], dup_index: usize) -> Vec<u64> {
        let mut ids = base.to_vec();
        let dup = ids[dup_index % ids.len()];
        ids.push(dup);
        let mut out = Vec::new(env);
        for id in ids {
            out.push_back(id);
        }
        out
    }

    fn assert_duplicate_rejected<T: core::fmt::Debug>(
        res: Result<
            Result<T, soroban_sdk::ConversionError>,
            Result<soroban_sdk::Error, soroban_sdk::InvokeError>,
        >,
    ) {
        let expected =
            soroban_sdk::Error::from_contract_error(ContractError::DuplicateSwapId as u32);
        match res {
            Err(Ok(err)) => assert_eq!(err, expected, "expected DuplicateSwapId"),
            other => panic!("duplicate batch must be rejected with DuplicateSwapId, got {other:?}"),
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn prop_execute_batch_swaps_rejects_duplicates(
            base in proptest::collection::vec(1u64..10_000, 1..8),
            dup_index in 0usize..8,
            atomic in any::<bool>(),
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(AtomicSwap, ());
            let client = AtomicSwapClient::new(&env, &contract_id);
            let ids = batch_with_duplicate(&env, &base, dup_index);
            assert_duplicate_rejected(client.try_execute_batch_swaps(&ids, &atomic));
        }

        #[test]
        fn prop_batch_timeout_auto_resolve_rejects_duplicates(
            base in proptest::collection::vec(1u64..10_000, 1..8),
            dup_index in 0usize..8,
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(AtomicSwap, ());
            let client = AtomicSwapClient::new(&env, &contract_id);
            let ids = batch_with_duplicate(&env, &base, dup_index);
            assert_duplicate_rejected(client.try_batch_timeout_auto_resolve(&ids));
        }

        #[test]
        fn prop_batch_accept_swaps_rejects_duplicates(
            base in proptest::collection::vec(1u64..10_000, 1..8),
            dup_index in 0usize..8,
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(AtomicSwap, ());
            let client = AtomicSwapClient::new(&env, &contract_id);
            let buyer = Address::generate(&env);
            let ids = batch_with_duplicate(&env, &base, dup_index);
            assert_duplicate_rejected(client.try_batch_accept_swaps(&ids, &buyer));
        }

        #[test]
        fn prop_batch_sign_swap_reveal_rejects_duplicates(
            base in proptest::collection::vec(1u64..10_000, 1..8),
            dup_index in 0usize..8,
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(AtomicSwap, ());
            let client = AtomicSwapClient::new(&env, &contract_id);
            let signer = Address::generate(&env);
            let ids = batch_with_duplicate(&env, &base, dup_index);
            assert_duplicate_rejected(client.try_batch_sign_swap_reveal(&ids, &signer));
        }

        #[test]
        fn prop_batch_escrow_deposit_rejects_duplicates(
            base in proptest::collection::vec(1u64..10_000, 1..8),
            dup_index in 0usize..8,
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(AtomicSwap, ());
            let client = AtomicSwapClient::new(&env, &contract_id);
            let buyer = Address::generate(&env);
            let ids = batch_with_duplicate(&env, &base, dup_index);
            assert_duplicate_rejected(client.try_batch_escrow_deposit(&ids, &buyer));
        }

        #[test]
        fn prop_batch_rollback_swaps_rejects_duplicates(
            base in proptest::collection::vec(1u64..10_000, 1..8),
            dup_index in 0usize..8,
        ) {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(AtomicSwap, ());
            let client = AtomicSwapClient::new(&env, &contract_id);
            let caller = Address::generate(&env);
            let reason = Bytes::from_slice(&env, b"prop");
            let ids = batch_with_duplicate(&env, &base, dup_index);
            assert_duplicate_rejected(client.try_batch_rollback_swaps(&ids, &caller, &reason));
        }
    }
}
