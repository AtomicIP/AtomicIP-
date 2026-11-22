//! Tests for the one-active-swap-per-IP invariant across auction and swap paths.
//! Issues #1331 (finalize_auction must lock the IP) and #1332 (start_ip_auction
//! must respect an existing active swap).

use ip_registry::{IpRegistry, IpRegistryClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    Address, BytesN, Env,
};

use crate::{AtomicSwap, AtomicSwapClient};

struct Fixture<'a> {
    env: Env,
    client: AtomicSwapClient<'a>,
    seller: Address,
    buyer: Address,
    token: Address,
    ip_id: u64,
}

fn setup() -> Fixture<'static> {
    let env = Env::default();
    env.mock_all_auths();

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let admin = Address::generate(&env);

    let registry_id = env.register(IpRegistry, ());
    let registry = IpRegistryClient::new(&env, &registry_id);
    let commitment = BytesN::from_array(&env, &[0x5A; 32]);
    let ip_id = registry.commit_ip(&seller, &commitment, &0);

    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    StellarAssetClient::new(&env, &token).mint(&buyer, &10_000);

    let contract_id = env.register(AtomicSwap, ());
    let client = AtomicSwapClient::new(&env, &contract_id);
    client.initialize(&admin, &registry_id);

    Fixture {
        env,
        client,
        seller,
        buyer,
        token,
        ip_id,
    }
}

#[test]
fn finalize_auction_locks_ip_against_new_swap() {
    let f = setup();

    let auction_id = f
        .client
        .start_ip_auction(&f.token, &f.ip_id, &f.seller, &100, &10, &0, &0);
    f.client.place_bid(&auction_id, &f.buyer, &200);

    f.env
        .ledger()
        .with_mut(|li| li.timestamp += 11);
    f.client.finalize_auction(&auction_id);

    // The auction-won swap is Accepted and must hold the IP lock, so a second
    // swap for the same IP through the regular path is rejected.
    let second = f.client.try_initiate_swap(
        &f.token, &f.ip_id, &f.seller, &500, &f.buyer, &0, &None, &0, &false,
    );
    assert!(second.is_err(), "IP must be locked by the auction-won swap");
}

#[test]
fn start_ip_auction_rejects_ip_with_active_swap() {
    let f = setup();

    f.client.initiate_swap(
        &f.token, &f.ip_id, &f.seller, &500, &f.buyer, &0, &None, &0, &false,
    );

    // The IP is already locked by a pending swap, so it must not be auctionable.
    let auction = f
        .client
        .try_start_ip_auction(&f.token, &f.ip_id, &f.seller, &100, &10, &0, &0);
    assert!(auction.is_err(), "auction must not start while a swap is active");
}
