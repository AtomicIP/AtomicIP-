//! Regression tests for batch and input validation hardening (#1339–#1342).

use ip_registry::{IpRegistry, IpRegistryClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token,
    token::StellarAssetClient,
    Address, Bytes, BytesN, Env, Error, Vec,
};

use crate::{AtomicSwap, AtomicSwapClient, ContractError, SwapCondition, SwapStatus};

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

// ── #1340: batch_reveal_keys enforces multi-signer and buyer conditions ───────

/// A swap that requires co-signers must not be releasable through the batch path
/// until every signer has signed, matching `reveal_key`.
#[test]
fn test_batch_reveal_keys_requires_all_signatures() {
    let env = Env::default();
    env.mock_all_auths();

    let seller = Address::generate(&env);
    let co_signer = Address::generate(&env);
    let buyer = Address::generate(&env);
    let admin = Address::generate(&env);

    let registry_id = setup_registry(&env);
    let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
    let contract_id = setup_contract(&env, &registry_id);
    let client = AtomicSwapClient::new(&env, &contract_id);
    let token = token::Client::new(&env, &token_id);

    let (ip_id, secret, blinding) = commit_ip(&env, &registry_id, &seller, 0x01);

    let mut signers = Vec::new(&env);
    signers.push_back(seller.clone());
    signers.push_back(co_signer.clone());
    let swap_id =
        client.initiate_swap_with_signers(&token_id, &ip_id, &seller, &1000i128, &buyer, &signers);
    client.accept_swap(&swap_id);

    let mut ids = Vec::new(&env);
    ids.push_back(swap_id);
    let mut secrets = Vec::new(&env);
    secrets.push_back(secret.clone());
    let mut blindings = Vec::new(&env);
    blindings.push_back(blinding.clone());

    // Only one of two signatures collected: batch reveal must be refused.
    client.sign_swap_reveal(&swap_id, &seller);
    let seller_before = token.balance(&seller);
    let contract_before = token.balance(&contract_id);
    let result = client.try_batch_reveal_keys(&ids, &secrets, &blindings, &seller);
    assert_eq!(
        result.unwrap_err(),
        contract_error(ContractError::NotAllSigned)
    );
    assert_eq!(token.balance(&seller), seller_before);
    assert_eq!(token.balance(&contract_id), contract_before);
    assert_eq!(
        client.get_swap(&swap_id).unwrap().status,
        SwapStatus::Accepted
    );

    // Final signature collected: batch reveal now succeeds.
    client.sign_swap_reveal(&swap_id, &co_signer);
    client.batch_reveal_keys(&ids, &secrets, &blindings, &seller);
    assert_eq!(
        client.get_swap(&swap_id).unwrap().status,
        SwapStatus::Completed
    );
}

/// Buyer conditions must be re-evaluated at reveal time in the batch path, as
/// `reveal_key` does. A `TimeAfter` condition that is not yet met must block the batch.
#[test]
fn test_batch_reveal_keys_enforces_buyer_conditions() {
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

    let (ip_id, secret, blinding) = commit_ip(&env, &registry_id, &seller, 0x02);

    let mut ip_ids = Vec::new(&env);
    ip_ids.push_back(ip_id);
    let mut prices = Vec::new(&env);
    prices.push_back(1000i128);
    let swap_ids =
        client.batch_initiate_swap(&token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None);
    let swap_id = swap_ids.get(0).unwrap();

    let mut conditions = Vec::new(&env);
    conditions.push_back(SwapCondition::TimeAfter(1_000u64));
    client.accept_swap_conditional(&swap_id, &conditions);

    let mut ids = Vec::new(&env);
    ids.push_back(swap_id);
    let mut secrets = Vec::new(&env);
    secrets.push_back(secret);
    let mut blindings = Vec::new(&env);
    blindings.push_back(blinding);

    // Ledger time is 0, before the TimeAfter(1000) condition: batch must be refused.
    let seller_before = token.balance(&seller);
    let contract_before = token.balance(&contract_id);
    let result = client.try_batch_reveal_keys(&ids, &secrets, &blindings, &seller);
    assert_eq!(
        result.unwrap_err(),
        contract_error(ContractError::ConditionNotMet)
    );
    assert_eq!(token.balance(&seller), seller_before);
    assert_eq!(token.balance(&contract_id), contract_before);

    // Once the condition holds, the same batch completes.
    env.ledger().with_mut(|l| l.timestamp = 1_000);
    client.batch_reveal_keys(&ids, &secrets, &blindings, &seller);
    assert_eq!(
        client.get_swap(&swap_id).unwrap().status,
        SwapStatus::Completed
    );
}

// ── #1341: every batch entry point rejects duplicate IDs ──────────────────────

mod duplicate_id_props {
    use proptest::prelude::*;
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

    use crate::{AtomicSwapClient, ContractError};

    use super::{contract_error, setup_contract, setup_registry};

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(24))]

        /// For any batch that contains a repeated ID, every batch entry point must
        /// refuse it with `DuplicateSwapId`. The IDs here do not refer to real swaps,
        /// so a result other than `DuplicateSwapId` would mean the duplicate check
        /// ran too late, after per-swap lookups.
        #[test]
        fn prop_batches_with_duplicate_ids_are_rejected(
            base in proptest::collection::vec(0u64..1_000, 1..=8),
        ) {
            // Force a duplicate: append a copy of the first ID.
            let mut raw = base.clone();
            raw.push(base[0]);

            let env = Env::default();
            env.mock_all_auths();

            let buyer = Address::generate(&env);
            let registry_id = setup_registry(&env);
            let contract_id = setup_contract(&env, &registry_id);
            let client = AtomicSwapClient::new(&env, &contract_id);

            let mut ids: Vec<u64> = Vec::new(&env);
            for id in raw.iter() {
                ids.push_back(*id);
            }
            let dup = contract_error(ContractError::DuplicateSwapId);

            prop_assert_eq!(
                client.try_execute_batch_swaps(&ids, &false).unwrap_err(),
                dup.clone()
            );
            prop_assert_eq!(
                client.try_execute_batch_swaps_prioritized(
                    &ids,
                    &false,
                    &{
                        let mut p = Vec::new(&env);
                        for _ in 0..ids.len() {
                            p.push_back(1u32);
                        }
                        p
                    }
                )
                .unwrap_err(),
                dup.clone()
            );
            prop_assert_eq!(
                client.try_batch_escrow_deposit(&ids, &buyer).unwrap_err(),
                dup.clone()
            );
            prop_assert_eq!(
                client.try_batch_sign_swap_reveal(&ids, &buyer).unwrap_err(),
                dup.clone()
            );
            prop_assert_eq!(
                client.try_batch_timeout_auto_resolve(&ids).unwrap_err(),
                dup.clone()
            );
            prop_assert_eq!(
                client
                    .try_batch_escrow_arbitration(
                        &ids,
                        &buyer,
                        &BytesN::from_array(&env, &[0u8; 32]),
                    )
                    .unwrap_err(),
                dup
            );
        }
    }
}

// ── #1342: initiate_swap rejects negative collateral ──────────────────────────

/// A negative `collateral_amount` must be refused with a structured error and must
/// not leave a swap record behind. The seller's IP and the token balances are
/// unaffected because the call panics before any state is written.
#[test]
fn test_initiate_swap_rejects_negative_collateral() {
    let env = Env::default();
    env.mock_all_auths();

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let admin = Address::generate(&env);

    let registry_id = setup_registry(&env);
    let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
    let contract_id = setup_contract(&env, &registry_id);
    let client = AtomicSwapClient::new(&env, &contract_id);

    let (ip_id, _secret, _blinding) = commit_ip(&env, &registry_id, &seller, 0x03);

    let result = client.try_initiate_swap(
        &token_id, &ip_id, &seller, &1000i128, &buyer, &0u32, &None, &-1i128, &false,
    );
    assert_eq!(
        result.unwrap_err(),
        contract_error(ContractError::InvalidCollateralAmount)
    );
    assert!(client.get_swap(&0u64).is_none());
}

/// Boundary: zero collateral is valid (it means no collateral is held), so the
/// same call with `0` must still create the swap.
#[test]
fn test_initiate_swap_accepts_zero_collateral() {
    let env = Env::default();
    env.mock_all_auths();

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let admin = Address::generate(&env);

    let registry_id = setup_registry(&env);
    let token_id = setup_token(&env, &admin, &buyer, 10_000_000);
    let contract_id = setup_contract(&env, &registry_id);
    let client = AtomicSwapClient::new(&env, &contract_id);

    let (ip_id, _secret, _blinding) = commit_ip(&env, &registry_id, &seller, 0x04);

    let swap_id = client.initiate_swap(
        &token_id, &ip_id, &seller, &1000i128, &buyer, &0u32, &None, &0i128, &false,
    );
    assert_eq!(
        client.get_swap(&swap_id).unwrap().status,
        SwapStatus::Pending
    );
}
