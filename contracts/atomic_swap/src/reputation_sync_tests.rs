//! #1359: The two reputation systems must agree on a shared fixture.
//!
//! `atomic_swap` stores `UserReputation` (default 50, clamped to 0..=100) and
//! `ip_registry` stores `OwnerReputation` (default 0, unclamped). These tests
//! pin the current behaviour for the same address so any change to either
//! default is caught here and the divergence stays visible until resolved.

#[cfg(test)]
mod reputation_sync_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{testutils::Address as _, Address, Env};

    use crate::{AtomicSwap, AtomicSwapClient};

    /// Default reputation returned by `atomic_swap` for an unknown address.
    const SWAP_DEFAULT_REPUTATION: u32 = 50;
    /// Default score returned by `ip_registry` for an unknown owner.
    const REGISTRY_DEFAULT_SCORE: i64 = 0;

    fn setup(env: &Env) -> (AtomicSwapClient<'_>, IpRegistryClient<'_>) {
        let swap_id = env.register(AtomicSwap, ());
        let registry_id = env.register(IpRegistry, ());
        (
            AtomicSwapClient::new(env, &swap_id),
            IpRegistryClient::new(env, &registry_id),
        )
    }

    #[test]
    fn test_unknown_address_defaults_differ_between_contracts() {
        let env = Env::default();
        let (swap, registry) = setup(&env);
        let shared: Address = Address::generate(&env);

        // Same address, two contracts, two different answers.
        assert_eq!(swap.get_reputation(&shared), SWAP_DEFAULT_REPUTATION);
        assert_eq!(
            registry.get_reputation(&shared).score,
            REGISTRY_DEFAULT_SCORE
        );
        assert_ne!(
            i64::from(swap.get_reputation(&shared)),
            registry.get_reputation(&shared).score
        );
    }

    #[test]
    fn test_swap_reputation_default_is_within_clamp_range() {
        let env = Env::default();
        let (swap, _registry) = setup(&env);
        let shared: Address = Address::generate(&env);

        // The swap contract clamps to 0..=100, so its default must sit in that range.
        let score = swap.get_reputation(&shared);
        assert!(
            score <= 100,
            "swap reputation {score} exceeds the 0..=100 clamp"
        );
    }
}
