//! #1087: Buyer-controlled escrow account tests — release, partial release,
//! dispute lock and admin resolution.

#[cfg(test)]
mod tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::{Client as TokenClient, StellarAssetClient},
        Address, Bytes, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus};

    const PRICE: i128 = 500;

    /// Holds the environment and ids. Clients are built per call via `client()`
    /// so no client outlives the `Env` it borrows.
    struct Fixture {
        env: Env,
        contract_id: Address,
        token_id: Address,
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

        fn balance(&self, who: &Address) -> i128 {
            TokenClient::new(&self.env, &self.token_id).balance(who)
        }
    }

    /// Registry, token, swap contract, and an escrow swap that the buyer has
    /// funded (status `Accepted`, deposit = `PRICE`).
    fn funded_escrow() -> Fixture {
        funded_escrow_with_price(PRICE)
    }

    fn funded_escrow_with_price(price: i128) -> Fixture {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[2u8; 32]);
        let blinding = BytesN::from_array(&env, &[3u8; 32]);
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

        let timeout = env.ledger().timestamp() + 100;
        let swap_id =
            client.initiate_escrow_swap(&token_id, &ip_id, &seller, &price, &buyer, &timeout);
        client.escrow_deposit(&swap_id);

        Fixture {
            env,
            contract_id,
            token_id,
            seller,
            buyer,
            admin,
            swap_id,
            secret,
            blinding,
        }
    }

    #[test]
    fn test_release_to_seller_pays_full_balance_and_completes() {
        let f = funded_escrow();
        assert_eq!(f.client().get_escrow_remaining(&f.swap_id), PRICE);

        assert!(f.client().release_to_seller(&f.swap_id));

        assert_eq!(f.balance(&f.seller), PRICE);
        assert_eq!(f.balance(&f.contract_id), 0);
        assert_eq!(f.client().get_escrow_remaining(&f.swap_id), 0);
        assert_eq!(
            f.client().get_swap(&f.swap_id).unwrap().status,
            SwapStatus::Completed
        );
    }

    #[test]
    fn test_partial_release_keeps_swap_accepted_until_fully_released() {
        let f = funded_escrow();

        assert!(f.client().release_partial_to_seller(&f.swap_id, &200));
        assert_eq!(f.balance(&f.seller), 200);
        assert_eq!(f.client().get_escrow_remaining(&f.swap_id), 300);
        assert_eq!(
            f.client().get_swap(&f.swap_id).unwrap().status,
            SwapStatus::Accepted
        );

        // Releasing the remainder completes the swap.
        assert!(f.client().release_to_seller(&f.swap_id));
        assert_eq!(f.balance(&f.seller), PRICE);
        assert_eq!(
            f.client().get_swap(&f.swap_id).unwrap().status,
            SwapStatus::Completed
        );
    }

    #[test]
    #[should_panic]
    fn test_partial_release_zero_amount_panics() {
        let f = funded_escrow();
        f.client().release_partial_to_seller(&f.swap_id, &0);
    }

    #[test]
    #[should_panic]
    fn test_partial_release_over_remaining_panics() {
        let f = funded_escrow();
        f.client()
            .release_partial_to_seller(&f.swap_id, &(PRICE + 1));
    }

    #[test]
    fn test_lock_blocks_release_until_resolved() {
        let f = funded_escrow();

        assert!(f.client().lock_escrow(&f.swap_id, &f.buyer));
        assert!(f.client().is_escrow_locked_for(&f.swap_id));
        // A second lock is a no-op.
        assert!(!f.client().lock_escrow(&f.swap_id, &f.seller));

        assert!(!f.client().release_to_seller(&f.swap_id));
        assert!(!f.client().release_partial_to_seller(&f.swap_id, &100));
        assert_eq!(f.balance(&f.seller), 0);
        assert_eq!(f.client().get_escrow_remaining(&f.swap_id), PRICE);
    }

    #[test]
    #[should_panic]
    fn test_withdraw_blocked_while_locked() {
        let f = funded_escrow();
        f.client().lock_escrow(&f.swap_id, &f.seller);
        f.env
            .ledger()
            .with_mut(|l| l.timestamp = l.timestamp + 1_000);
        f.client().escrow_withdraw(&f.swap_id);
    }

    #[test]
    fn test_admin_early_release_resolves_dispute_to_seller() {
        let f = funded_escrow();
        f.client().lock_escrow(&f.swap_id, &f.buyer);

        assert!(f
            .client()
            .resolve_escrow_dispute(&f.swap_id, &f.admin, &true));

        assert!(!f.client().is_escrow_locked_for(&f.swap_id));
        assert_eq!(f.balance(&f.seller), PRICE);
        assert_eq!(
            f.client().get_swap(&f.swap_id).unwrap().status,
            SwapStatus::Completed
        );
    }

    #[test]
    fn test_admin_refund_resolves_dispute_to_buyer() {
        let f = funded_escrow();
        f.client().lock_escrow(&f.swap_id, &f.seller);
        let buyer_before = f.balance(&f.buyer);

        assert!(f
            .client()
            .resolve_escrow_dispute(&f.swap_id, &f.admin, &false));

        assert_eq!(f.balance(&f.buyer), buyer_before + PRICE);
        assert_eq!(f.balance(&f.seller), 0);
        assert_eq!(
            f.client().get_swap(&f.swap_id).unwrap().status,
            SwapStatus::Cancelled
        );
    }

    #[test]
    fn test_resolve_without_lock_returns_false() {
        let f = funded_escrow();
        assert!(!f
            .client()
            .resolve_escrow_dispute(&f.swap_id, &f.admin, &true));
        assert_eq!(f.balance(&f.seller), 0);
    }

    #[test]
    #[should_panic]
    fn test_resolve_by_non_admin_panics() {
        let f = funded_escrow();
        f.client().lock_escrow(&f.swap_id, &f.buyer);
        let outsider = Address::generate(&f.env);
        f.client()
            .resolve_escrow_dispute(&f.swap_id, &outsider, &true);
    }

    #[test]
    fn test_reveal_after_partial_release_pays_only_remaining_balance() {
        // A price below 40 keeps the 2.5% protocol fee at zero, so reveal does not
        // transfer to the placeholder treasury (which has no trustline in tests).
        let price: i128 = 39;
        let f = funded_escrow_with_price(price);
        assert!(f.client().release_partial_to_seller(&f.swap_id, &9));

        f.client()
            .reveal_key(&f.swap_id, &f.seller, &f.secret, &f.blinding);

        // Seller received exactly the full price across both payouts; the
        // contract holds nothing afterwards (no double payment).
        assert_eq!(f.balance(&f.seller), price);
        assert_eq!(f.balance(&f.contract_id), 0);
        assert_eq!(
            f.client().get_swap(&f.swap_id).unwrap().status,
            SwapStatus::Completed
        );
    }

    #[test]
    #[should_panic]
    fn test_release_on_atomic_swap_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let commitment_hash = BytesN::from_array(&env, &[9u8; 32]);
        let ip_id = registry.commit_ip(&seller, &commitment_hash, &0u32);
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(&env, &token_id).mint(&buyer, &1_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &PRICE, &buyer, &0u32, &None, &0i128, &false,
        );
        client.accept_swap(&swap_id);

        client.release_to_seller(&swap_id);
    }
}
