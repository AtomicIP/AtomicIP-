use ed25519_dalek::{Signer, SigningKey};
use ip_registry::{IpRegistry, IpRegistryClient};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    xdr::ToXdr,
    Address, Bytes, BytesN, Env, Symbol,
};

use crate::price_oracle::{PriceAttestation, SignedPrice};
use crate::{
    AtomicSwap, AtomicSwapClient, ContractError, MarketPriceContingency, OracleEventContingency,
    SwapContingency, SwapStatus, TimeWindowContingency,
};

#[contract]
pub struct ContingencyOracle;

#[contractimpl]
impl ContingencyOracle {
    pub fn get_price_attestation(env: Env, _token: Address) -> SignedPrice {
        env.storage()
            .instance()
            .get::<Symbol, SignedPrice>(&Symbol::new(&env, "signed"))
            .unwrap()
    }

    pub fn set_signed_price(env: Env, price: i128, timestamp: u64, signature: BytesN<64>) {
        env.storage().instance().set(
            &Symbol::new(&env, "signed"),
            &SignedPrice {
                price,
                timestamp,
                signature,
            },
        );
    }
}

fn setup_swap() -> (
    Env,
    AtomicSwapClient<'static>,
    u64,
    Address,
    Address,
    Address,
    BytesN<32>,
    BytesN<32>,
) {
    let env = Env::default();
    env.mock_all_auths();
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let admin = Address::generate(&env);
    let registry_id = env.register(IpRegistry, ());
    let registry = IpRegistryClient::new(&env, &registry_id);
    let secret = BytesN::from_array(&env, &[0xA1; 32]);
    let blinding = BytesN::from_array(&env, &[0xB2; 32]);
    let mut preimage = Bytes::new(&env);
    preimage.append(&Bytes::from(secret.clone()));
    preimage.append(&Bytes::from(blinding.clone()));
    let commitment: BytesN<32> = env.crypto().sha256(&preimage).into();
    let ip_id = registry.commit_ip(&seller, &commitment, &0);
    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    StellarAssetClient::new(&env, &token_id).mint(&buyer, &10_000);
    let contract_id = env.register(AtomicSwap, ());
    let client = AtomicSwapClient::new(&env, &contract_id);
    client.initialize(&registry_id);
    let swap_id = client.initiate_swap(
        &token_id, &ip_id, &seller, &500, &buyer, &0, &None, &0, &false,
    );
    (
        env, client, swap_id, seller, buyer, token_id, secret, blinding,
    )
}

fn configure_oracle(env: &Env, client: &AtomicSwapClient, seller: &Address, token: &Address) {
    let oracle_id = env.register(ContingencyOracle, ());
    let oracle = ContingencyOracleClient::new(env, &oracle_id);
    let signing_key = SigningKey::from_bytes(&[0x42; 32]);
    let pubkey = BytesN::from_array(env, &signing_key.verifying_key().to_bytes());
    client.set_oracle(seller, &oracle_id, &pubkey, &true, &0);

    let price = 500_i128;
    let timestamp = env.ledger().timestamp();
    let attestation = PriceAttestation {
        token: token.clone(),
        price,
        timestamp,
    };
    let signature = signing_key.sign(&attestation.to_xdr(env).to_alloc_vec());
    oracle.set_signed_price(
        &price,
        &timestamp,
        &BytesN::from_array(env, &signature.to_bytes()),
    );
}

#[test]
fn contingencies_form_an_ordered_chain_and_validate_all_types() {
    let (env, client, swap_id, seller, _buyer, token, _, _) = setup_swap();
    configure_oracle(&env, &client, &seller, &token);
    let now = env.ledger().timestamp();

    let id0 = client.add_contingency(
        &swap_id,
        &SwapContingency::MarketPrice(MarketPriceContingency {
            token: token.clone(),
            min_price: 450,
            max_price: 550,
        })
        .to_xdr(&env),
    );
    let id1 = client.add_contingency(
        &swap_id,
        &SwapContingency::OracleEvent(OracleEventContingency { token, price: 500 }).to_xdr(&env),
    );
    let id2 = client.add_contingency(
        &swap_id,
        &SwapContingency::TimeWindow(TimeWindowContingency {
            start: now,
            end: now + 1_000,
        })
        .to_xdr(&env),
    );

    assert_eq!((id0, id1, id2), (0, 1, 2));
    let chain = client.get_contingencies(&swap_id);
    assert_eq!(chain.len(), 3);
    assert_eq!(chain.get(0).unwrap().previous_id, None);
    assert_eq!(chain.get(1).unwrap().previous_id, Some(0));
    assert_eq!(chain.get(2).unwrap().previous_id, Some(1));

    client.accept_swap(&swap_id);
    assert_eq!(
        client.get_swap(&swap_id).unwrap().status,
        SwapStatus::Accepted
    );
}

#[test]
fn failed_time_window_keeps_swap_pending() {
    let (env, client, swap_id, _seller, _buyer, _token, _, _) = setup_swap();
    let now = env.ledger().timestamp();
    client.add_contingency(
        &swap_id,
        &SwapContingency::TimeWindow(TimeWindowContingency {
            start: now,
            end: now + 10,
        })
        .to_xdr(&env),
    );
    env.ledger().with_mut(|ledger| ledger.timestamp = now + 11);

    let result = client.try_accept_swap(&swap_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        soroban_sdk::Error::from_contract_error(ContractError::ConditionNotMet as u32)
    );
    assert_eq!(
        client.get_swap(&swap_id).unwrap().status,
        SwapStatus::Pending
    );
}

#[test]
fn market_price_outside_range_keeps_swap_pending() {
    let (env, client, swap_id, seller, _buyer, token, _, _) = setup_swap();
    configure_oracle(&env, &client, &seller, &token);
    client.add_contingency(
        &swap_id,
        &SwapContingency::MarketPrice(MarketPriceContingency {
            token,
            min_price: 600,
            max_price: 700,
        })
        .to_xdr(&env),
    );

    let result = client.try_accept_swap(&swap_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        soroban_sdk::Error::from_contract_error(ContractError::ConditionNotMet as u32)
    );
    assert_eq!(
        client.get_swap(&swap_id).unwrap().status,
        SwapStatus::Pending
    );
}

#[test]
fn contingency_is_rechecked_before_key_settlement() {
    let (env, client, swap_id, seller, _buyer, _token, secret, blinding) = setup_swap();
    let now = env.ledger().timestamp();
    client.add_contingency(
        &swap_id,
        &SwapContingency::TimeWindow(TimeWindowContingency {
            start: now,
            end: now + 10,
        })
        .to_xdr(&env),
    );
    client.accept_swap(&swap_id);
    env.ledger().with_mut(|ledger| ledger.timestamp = now + 11);

    let result = client.try_reveal_key(&swap_id, &seller, &secret, &blinding);
    assert_eq!(
        result.unwrap_err().unwrap(),
        soroban_sdk::Error::from_contract_error(ContractError::ConditionNotMet as u32)
    );
    assert_eq!(
        client.get_swap(&swap_id).unwrap().status,
        SwapStatus::Accepted
    );
}
