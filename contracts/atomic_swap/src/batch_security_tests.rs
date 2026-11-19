//! Regression tests for batch and input validation hardening (#1339–#1342).

use ip_registry::{IpRegistry, IpRegistryClient};
use soroban_sdk::{
    testutils::Address as _, token, token::StellarAssetClient, Address, Bytes, BytesN, Env, Error,
    Vec,
};

use crate::{AtomicSwap, AtomicSwapClient, ContractError, SwapStatus};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn setup_registry(env: &Env) -> Address {
    env.register(IpRegistry, ())
}

fn commit_ip(
    env: &Env,
    registry_id: &Address,
    owner: &Address,
    seed: u8,
) -> (u64, BytesN<32>, BytesN<32>) {
    let registry = IpRegistryClient::new(env, registry_id);
    let secret = BytesN::from_array(env, &[seed; 32]);
    let blinding = BytesN::from_array(env, &[seed.wrapping_add(0x80); 32]);
    let mut preimage = Bytes::new(env);
    preimage.append(&Bytes::from(secret.clone()));
    preimage.append(&Bytes::from(blinding.clone()));
    let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
    let ip_id = registry.commit_ip(owner, &hash, &0u32);
    (ip_id, secret, blinding)
}

fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let sac = StellarAssetClient::new(env, &token_id);
    sac.mint(recipient, &amount);
    // Establish a balance entry for the hardcoded protocol treasury so the
    // protocol-fee transfer in batch_reveal_keys succeeds in the test environment.
    let treasury = Address::from_string(&soroban_sdk::String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ));
    sac.mint(&treasury, &0);
    token_id
}

fn setup_contract(env: &Env, registry_id: &Address) -> Address {
    let contract_id = env.register(AtomicSwap, ());
    let deployer = Address::generate(env);
    AtomicSwapClient::new(env, &contract_id).initialize(&deployer, registry_id);
    contract_id
}

fn contract_error(code: ContractError) -> Result<Error, soroban_sdk::InvokeError> {
    Ok(Error::from_contract_error(code as u32))
}

// ── #1339: batch_reveal_keys rejects duplicate swap IDs ───────────────────────

/// Two swaps are accepted, so the contract holds both prices. Revealing
/// `[id0, id0]` must panic with `DuplicateSwapId` and move no funds: before the
/// fix the payout for `id0` was released twice out of the shared balance.
#[test]
fn test_batch_reveal_keys_duplicate_id_panics_without_state_change() {
    let env = Env::default();
    env.mock_all_auths();

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let admin = Address::generate(&env);

    let registry_id = setup_registry(&env);
    let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
    let contract_id = setup_contract(&env, &registry_id);
    let client = AtomicSwapClient::new(&env, &contract_id);
    let token = token::Client::new(&env, &token_id);

    let (ip1, s1, b1) = commit_ip(&env, &registry_id, &seller, 0x01);
    let (ip2, _s2, _b2) = commit_ip(&env, &registry_id, &seller, 0x02);

    let mut ip_ids = Vec::new(&env);
    ip_ids.push_back(ip1);
    ip_ids.push_back(ip2);
    let mut prices = Vec::new(&env);
    prices.push_back(1000i128);
    prices.push_back(1000i128);

    let swap_ids =
        client.batch_initiate_swap(&token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None);
    let id0 = swap_ids.get(0).unwrap();
    let id1 = swap_ids.get(1).unwrap();

    let mut accept_ids = Vec::new(&env);
    accept_ids.push_back(id0);
    accept_ids.push_back(id1);
    client.batch_accept_swaps(&accept_ids, &buyer);

    let seller_before = token.balance(&seller);
    let contract_before = token.balance(&contract_id);

    let mut dup_ids = Vec::new(&env);
    dup_ids.push_back(id0);
    dup_ids.push_back(id0);
    let mut secrets = Vec::new(&env);
    secrets.push_back(s1.clone());
    secrets.push_back(s1);
    let mut blindings = Vec::new(&env);
    blindings.push_back(b1.clone());
    blindings.push_back(b1);

    let result = client.try_batch_reveal_keys(&dup_ids, &secrets, &blindings, &seller);
    assert_eq!(
        result.unwrap_err(),
        contract_error(ContractError::DuplicateSwapId)
    );

    assert_eq!(token.balance(&seller), seller_before);
    assert_eq!(token.balance(&contract_id), contract_before);
    assert_eq!(client.get_swap(&id0).unwrap().status, SwapStatus::Accepted);
}
