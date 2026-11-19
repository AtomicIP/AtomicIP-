//! #1350: `create_liquidity_pool` must verify the IP being pooled against
//! `ip_registry`: the commitment must exist, must not be revoked, and must be
//! owned by the admin. Failed creations must leave no pool and move no tokens.

#[cfg(test)]
mod liquidity_pool_ip_verification_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _,
        token::{Client as TokenClient, StellarAssetClient},
        Address, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient};

    const IP_RESERVE: u64 = 1_000;
    const TOKEN_RESERVE: i128 = 10_000;
    const MAX_SLIPPAGE_BPS: u32 = 1_000;
    const UNKNOWN_COMMITMENT: u64 = 999;

    struct Fixture {
        env: Env,
        contract_id: Address,
        registry_id: Address,
        admin: Address,
        token_id: Address,
    }

    impl Fixture {
        fn client(&self) -> AtomicSwapClient<'_> {
            AtomicSwapClient::new(&self.env, &self.contract_id)
        }

        fn registry(&self) -> IpRegistryClient<'_> {
            IpRegistryClient::new(&self.env, &self.registry_id)
        }

        fn token(&self) -> TokenClient<'_> {
            TokenClient::new(&self.env, &self.token_id)
        }

        /// Commits a new IP to the registry on behalf of `owner`.
        fn commit(&self, owner: &Address, seed: u8) -> u64 {
            let hash = BytesN::from_array(&self.env, &[seed; 32]);
            self.registry().commit_ip(owner, &hash, &0u32)
        }

        /// Returns true only if the admin's pool creation succeeds.
        fn create_pool_succeeds(&self, commitment_id: u64) -> bool {
            matches!(
                self.client().try_create_liquidity_pool(
                    &self.admin,
                    &commitment_id,
                    &self.token_id,
                    &IP_RESERVE,
                    &TOKEN_RESERVE,
                    &MAX_SLIPPAGE_BPS,
                ),
                Ok(Ok(()))
            )
        }
    }

    /// Deploys the registry, the AtomicSwap contract and a funded admin token balance.
    fn setup() -> Fixture {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let registry_id = env.register(IpRegistry, ());

        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(&env, &token_id).mint(&admin, &TOKEN_RESERVE);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&admin, &registry_id);
        client.set_admin(&admin);

        Fixture {
            env,
            contract_id,
            registry_id,
            admin,
            token_id,
        }
    }

    // ── Control: an admin-owned, live commitment can be pooled ────────────────

    #[test]
    fn test_pool_created_for_admin_owned_live_commitment() {
        let f = setup();
        let commitment = f.commit(&f.admin, 0x50);

        assert!(f.create_pool_succeeds(commitment));
        assert!(f.client().get_liquidity_pool(&commitment).is_some());
        assert_eq!(f.token().balance(&f.contract_id), TOKEN_RESERVE);
    }

    // ── Rejections ────────────────────────────────────────────────────────────

    #[test]
    fn test_pool_rejected_for_nonexistent_commitment() {
        let f = setup();
        let balance_before = f.token().balance(&f.admin);

        assert!(!f.create_pool_succeeds(UNKNOWN_COMMITMENT));

        assert!(f.client().get_liquidity_pool(&UNKNOWN_COMMITMENT).is_none());
        assert_eq!(f.token().balance(&f.admin), balance_before);
    }

    #[test]
    fn test_pool_rejected_for_revoked_commitment() {
        let f = setup();
        let commitment = f.commit(&f.admin, 0x51);
        f.registry().revoke_ip(&commitment);
        let balance_before = f.token().balance(&f.admin);

        assert!(!f.create_pool_succeeds(commitment));

        assert!(f.client().get_liquidity_pool(&commitment).is_none());
        assert_eq!(f.token().balance(&f.admin), balance_before);
    }

    #[test]
    fn test_pool_rejected_for_commitment_owned_by_someone_else() {
        let f = setup();
        let other_owner = Address::generate(&f.env);
        let commitment = f.commit(&other_owner, 0x52);
        let balance_before = f.token().balance(&f.admin);

        assert!(!f.create_pool_succeeds(commitment));

        assert!(f.client().get_liquidity_pool(&commitment).is_none());
        assert_eq!(f.token().balance(&f.admin), balance_before);
    }
}
