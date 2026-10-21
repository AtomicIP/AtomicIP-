//! #1089: Settlement finality tests — settlement clock, multi-ledger finality,
//! admin-configured threshold and finality callbacks.

#[cfg(test)]
mod tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        contract, contractimpl, symbol_short,
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Bytes, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus, DEFAULT_FINALITY_BLOCKS};

    const PRICE: i128 = 500;

    /// Callback target. A contract cannot call itself, so callbacks go to this
    /// separate contract, which counts how many times it was invoked.
    #[contract]
    pub struct FinalityReceiver;

    #[contractimpl]
    impl FinalityReceiver {
        pub fn on_final(env: Env, swap_id: u64) {
            let hits: u32 = env.storage().instance().get(&symbol_short!("hits")).unwrap_or(0);
            env.storage().instance().set(&symbol_short!("hits"), &(hits + 1));
            env.storage().instance().set(&symbol_short!("last"), &swap_id);
        }

        pub fn hits(env: Env) -> u32 {
            env.storage().instance().get(&symbol_short!("hits")).unwrap_or(0)
        }

        pub fn last(env: Env) -> u64 {
            env.storage().instance().get(&symbol_short!("last")).unwrap_or(0)
        }
    }

    struct Fixture {
        env: Env,
        contract_id: Address,
        seller: Address,
        buyer: Address,
        admin: Address,
        swap_id: u64,
        secret: BytesN<32>,
        blinding: BytesN<32>,
    }

    impl Fixture {
        fn client(&self) -> AtomicSwapClient<'_> {
            AtomicSwapClient::new(&self.env, &self.contract_id)
        }

        fn advance_ledgers(&self, n: u32) {
            self.env
                .ledger()
                .with_mut(|l| l.sequence_number += n);
        }
    }

    /// An escrow swap that is funded and ready to reveal, but not yet revealed.
    fn funded_unrevealed_swap() -> Fixture {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[4u8; 32]);
        let blinding = BytesN::from_array(&env, &[5u8; 32]);
        let mut preimage = Bytes::new(&env);
        preimage.append(&Bytes::from(secret.clone()));
        preimage.append(&Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &commitment_hash, &0u32);

        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(&env, &token_id).mint(&buyer, &1_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);
        client.set_admin(&admin);

        let timeout = env.ledger().timestamp() + 1_000;
        let swap_id =
            client.initiate_escrow_swap(&token_id, &ip_id, &seller, &PRICE, &buyer, &timeout);
        client.escrow_deposit(&swap_id);

        Fixture {
            env,
            contract_id,
            seller,
            buyer,
            admin,
            swap_id,
            secret,
            blinding,
        }
    }

    /// A swap that has settled. Settles through the buyer's full escrow release,
    /// which records settlement without the protocol fee that `reveal_key` pays
    /// to the placeholder treasury.
    fn settled_swap() -> Fixture {
        let f = funded_unrevealed_swap();
        assert!(f.client().release_to_seller(&f.swap_id));
        f
    }

    #[test]
    fn test_settlement_clock_starts_at_zero_and_is_not_final() {
        let f = settled_swap();
        assert_eq!(f.client().get_settlement_finality(&f.swap_id), 0);
        assert!(!f.client().is_settlement_final(&f.swap_id));
        assert_eq!(
            f.client().get_swap(&f.swap_id).unwrap().status,
            SwapStatus::Completed
        );
    }

    #[test]
    fn test_unsettled_swap_reports_zero_finality() {
        let f = funded_unrevealed_swap();
        assert_eq!(f.client().get_settlement_finality(&f.swap_id), 0);
        assert!(!f.client().is_settlement_final(&f.swap_id));
    }

    #[test]
    fn test_finality_reached_after_default_block_count() {
        let f = settled_swap();

        f.advance_ledgers(DEFAULT_FINALITY_BLOCKS - 1);
        assert!(!f.client().is_settlement_final(&f.swap_id));

        f.advance_ledgers(1);
        assert_eq!(
            f.client().get_settlement_finality(&f.swap_id),
            DEFAULT_FINALITY_BLOCKS as u64
        );
        assert!(f.client().is_settlement_final(&f.swap_id));
    }

    #[test]
    fn test_admin_can_change_finality_blocks() {
        let f = settled_swap();
        f.client().set_finality_blocks(&f.admin, &2);

        f.advance_ledgers(2);
        assert!(f.client().is_settlement_final(&f.swap_id));
    }

    #[test]
    #[should_panic]
    fn test_non_admin_cannot_set_finality_blocks() {
        let f = settled_swap();
        let outsider = Address::generate(&f.env);
        f.client().set_finality_blocks(&outsider, &2);
    }

    #[test]
    #[should_panic]
    fn test_zero_finality_blocks_rejected() {
        let f = settled_swap();
        f.client().set_finality_blocks(&f.admin, &0);
    }

    #[test]
    #[should_panic]
    fn test_register_callback_before_settlement_panics() {
        let f = funded_unrevealed_swap();
        f.client().register_finality_callback(
            &f.swap_id,
            &f.buyer,
            &f.contract_id,
            &symbol_short!("on_final"),
        );
    }

    #[test]
    #[should_panic]
    fn test_outsider_cannot_register_callback() {
        let f = settled_swap();
        let outsider = Address::generate(&f.env);
        f.client().register_finality_callback(
            &f.swap_id,
            &outsider,
            &f.contract_id,
            &symbol_short!("on_final"),
        );
    }

    #[test]
    #[should_panic]
    fn test_execute_callbacks_before_final_panics() {
        let f = settled_swap();
        f.client().register_finality_callback(
            &f.swap_id,
            &f.seller,
            &f.contract_id,
            &symbol_short!("on_final"),
        );
        f.client().execute_finality_callbacks(&f.swap_id);
    }

    #[test]
    fn test_callbacks_run_once_after_finality() {
        let f = settled_swap();
        let receiver = f.env.register(FinalityReceiver, ());
        let receiver_client = FinalityReceiverClient::new(&f.env, &receiver);

        f.client().register_finality_callback(
            &f.swap_id,
            &f.seller,
            &receiver,
            &symbol_short!("on_final"),
        );
        f.client().register_finality_callback(
            &f.swap_id,
            &f.buyer,
            &receiver,
            &symbol_short!("on_final"),
        );

        f.advance_ledgers(DEFAULT_FINALITY_BLOCKS);
        assert_eq!(f.client().execute_finality_callbacks(&f.swap_id), 2);
        assert_eq!(receiver_client.hits(), 2);
        assert_eq!(receiver_client.last(), f.swap_id);

        // Callbacks are cleared once executed, so a second run invokes nothing.
        assert_eq!(f.client().execute_finality_callbacks(&f.swap_id), 0);
        assert_eq!(receiver_client.hits(), 2);
    }

    #[test]
    fn test_no_callbacks_registered_returns_zero() {
        let f = settled_swap();
        f.advance_ledgers(DEFAULT_FINALITY_BLOCKS);
        assert_eq!(f.client().execute_finality_callbacks(&f.swap_id), 0);
    }

    #[test]
    #[should_panic]
    fn test_callback_limit_enforced() {
        let f = settled_swap();
        for _ in 0..=crate::MAX_FINALITY_CALLBACKS {
            f.client().register_finality_callback(
                &f.swap_id,
                &f.seller,
                &f.contract_id,
                &symbol_short!("on_final"),
            );
        }
    }
}
