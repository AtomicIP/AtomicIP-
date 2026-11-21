//! Tests for arbitration voting, finalization and stake ownership.
//!
//! * #1367: `transfer_ip` migrates the stake; only the new owner can unstake.
//! * #1368: an arbitrator can vote only once per case.
//! * #1369: `finalize_arbitration` cannot overwrite a dispute already resolved
//!   by `resolve_dispute` (and vice versa).
//! * #1370: `finalize_arbitration` requires a quorum; ties go to the owner.

use crate::{ContractError, IpRegistry, IpRegistryClient};
use soroban_sdk::testutils::{Address as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{Address, BytesN, Env, IntoVal};

struct Setup {
    env: Env,
    client: IpRegistryClient<'static>,
    contract_id: Address,
    owner: Address,
    challenger: Address,
    ip_id: u64,
    dispute_id: u64,
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(IpRegistry, ());
    let client = IpRegistryClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let owner = Address::generate(&env);
    let challenger = Address::generate(&env);
    let ip_id = client.commit_ip(&owner, &BytesN::from_array(&env, &[0x42u8; 32]), &0u32);
    let dispute_id =
        client.initiate_dispute(&ip_id, &challenger, &BytesN::from_array(&env, &[0x07u8; 32]));

    Setup { env, client, contract_id, owner, challenger, ip_id, dispute_id }
}

/// Nominate `n` arbitrators and return them.
fn nominate(s: &Setup, n: u32) -> soroban_sdk::Vec<Address> {
    let mut out = soroban_sdk::Vec::new(&s.env);
    for _ in 0..n {
        let a = Address::generate(&s.env);
        s.client.nominate_arbitrator(&a);
        out.push_back(a);
    }
    out
}

fn assert_contract_error<T: core::fmt::Debug, E: core::fmt::Debug>(
    result: Result<T, Result<soroban_sdk::Error, E>>,
    expected: ContractError,
) {
    match result {
        Err(Ok(err)) => assert_eq!(err, soroban_sdk::Error::from_contract_error(expected as u32)),
        other => panic!("expected {:?}, got {:?}", expected, other),
    }
}

// ── #1367 ────────────────────────────────────────────────────────────────────

#[test]
fn transfer_migrates_stake_owner() {
    let s = setup();
    s.client.stake_commitment(&s.ip_id, &100i128);
    let new_owner = Address::generate(&s.env);
    s.client.transfer_ip(&s.ip_id, &new_owner);

    let stake = s.client.get_stake(&s.ip_id).unwrap();
    assert_eq!(stake.owner, new_owner);
}

#[test]
fn after_transfer_new_owner_can_unstake() {
    let s = setup();
    s.client.stake_commitment(&s.ip_id, &100i128);
    let new_owner = Address::generate(&s.env);
    s.client.transfer_ip(&s.ip_id, &new_owner);

    s.env.mock_auths(&[MockAuth {
        address: &new_owner,
        invoke: &MockAuthInvoke {
            contract: &s.contract_id,
            fn_name: "unstake",
            args: (s.ip_id,).into_val(&s.env),
            sub_invokes: &[],
        },
    }]);
    s.client.unstake(&s.ip_id);
    assert!(s.client.get_stake(&s.ip_id).is_none());
}

#[test]
#[should_panic]
fn after_transfer_former_owner_cannot_unstake() {
    let s = setup();
    s.client.stake_commitment(&s.ip_id, &100i128);
    let new_owner = Address::generate(&s.env);
    s.client.transfer_ip(&s.ip_id, &new_owner);

    // Only the former owner signs; the contract must demand the new owner.
    s.env.mock_auths(&[MockAuth {
        address: &s.owner,
        invoke: &MockAuthInvoke {
            contract: &s.contract_id,
            fn_name: "unstake",
            args: (s.ip_id,).into_val(&s.env),
            sub_invokes: &[],
        },
    }]);
    s.client.unstake(&s.ip_id);
}

#[test]
fn slash_after_transfer_penalises_new_owner() {
    let s = setup();
    s.client.stake_commitment(&s.ip_id, &100i128);
    let new_owner = Address::generate(&s.env);
    s.client.transfer_ip(&s.ip_id, &new_owner);

    s.client.slash_stake(&s.ip_id);
    assert_eq!(s.client.get_reputation(&new_owner).disputes_lost, 1);
    assert_eq!(s.client.get_reputation(&s.owner).disputes_lost, 0);
}

// ── #1368 ────────────────────────────────────────────────────────────────────

#[test]
fn arbitrator_second_vote_rejected() {
    let s = setup();
    let arbs = nominate(&s, 3);
    let arb_id = s.client.open_arbitration(&s.dispute_id);
    let a = arbs.get(0).unwrap();

    s.client.vote_on_dispute(&arb_id, &a, &false);
    let res = s.client.try_vote_on_dispute(&arb_id, &a, &false);
    assert_contract_error(res, ContractError::AlreadyVoted);

    let case = s.client.get_arbitration(&arb_id);
    assert_eq!(case.votes_challenger, 1);
    assert_eq!(case.votes_owner, 0);
}

#[test]
#[should_panic]
fn arbitrator_second_vote_panics_even_when_switching_sides() {
    let s = setup();
    let arbs = nominate(&s, 3);
    let arb_id = s.client.open_arbitration(&s.dispute_id);
    let a = arbs.get(0).unwrap();

    s.client.vote_on_dispute(&arb_id, &a, &true);
    s.client.vote_on_dispute(&arb_id, &a, &false);
}

#[test]
fn votes_are_per_case() {
    let s = setup();
    let arbs = nominate(&s, 1);
    let a = arbs.get(0).unwrap();
    let dispute2 = s.client.initiate_dispute(
        &s.ip_id,
        &s.challenger,
        &BytesN::from_array(&s.env, &[0x08u8; 32]),
    );
    let arb1 = s.client.open_arbitration(&s.dispute_id);
    let arb2 = s.client.open_arbitration(&dispute2);

    s.client.vote_on_dispute(&arb1, &a, &true);
    s.client.vote_on_dispute(&arb2, &a, &true);
}

// ── #1369 ────────────────────────────────────────────────────────────────────

#[test]
fn resolve_then_finalize_rejected() {
    let s = setup();
    let arbs = nominate(&s, 1);
    let arb_id = s.client.open_arbitration(&s.dispute_id);
    s.client.vote_on_dispute(&arb_id, &arbs.get(0).unwrap(), &false);

    s.client.resolve_dispute(&s.dispute_id, &s.owner);
    let res = s.client.try_finalize_arbitration(&arb_id);
    assert_contract_error(res, ContractError::DisputeAlreadyResolved);

    let dispute = s.client.get_dispute(&s.dispute_id);
    assert_eq!(dispute.winner, Some(s.owner.clone()));
    assert!(!s.client.get_arbitration(&arb_id).finalized);
}

#[test]
fn finalize_then_resolve_rejected() {
    let s = setup();
    let arbs = nominate(&s, 1);
    let arb_id = s.client.open_arbitration(&s.dispute_id);
    s.client.vote_on_dispute(&arb_id, &arbs.get(0).unwrap(), &false);

    s.client.finalize_arbitration(&arb_id);
    let res = s.client.try_resolve_dispute(&s.dispute_id, &s.owner);
    assert_contract_error(res, ContractError::DisputeAlreadyResolved);

    let dispute = s.client.get_dispute(&s.dispute_id);
    assert!(dispute.resolved);
    assert_eq!(dispute.winner, Some(s.challenger.clone()));
}

#[test]
fn open_arbitration_on_resolved_dispute_rejected() {
    let s = setup();
    nominate(&s, 1);
    s.client.resolve_dispute(&s.dispute_id, &s.owner);
    let res = s.client.try_open_arbitration(&s.dispute_id);
    assert_contract_error(res, ContractError::DisputeAlreadyResolved);
}

// ── #1370 ────────────────────────────────────────────────────────────────────

#[test]
fn zero_votes_cannot_finalize() {
    let s = setup();
    nominate(&s, 3);
    let arb_id = s.client.open_arbitration(&s.dispute_id);
    let res = s.client.try_finalize_arbitration(&arb_id);
    assert_contract_error(res, ContractError::ArbitrationQuorumNotMet);
    assert!(!s.client.get_dispute(&s.dispute_id).resolved);
}

#[test]
fn no_arbitrators_cannot_finalize() {
    let s = setup();
    let arb_id = s.client.open_arbitration(&s.dispute_id);
    let res = s.client.try_finalize_arbitration(&arb_id);
    assert_contract_error(res, ContractError::ArbitrationQuorumNotMet);
}

#[test]
fn below_quorum_cannot_finalize() {
    let s = setup();
    let arbs = nominate(&s, 5);
    let arb_id = s.client.open_arbitration(&s.dispute_id);
    // quorum for 5 arbitrators is 3
    s.client.vote_on_dispute(&arb_id, &arbs.get(0).unwrap(), &false);
    s.client.vote_on_dispute(&arb_id, &arbs.get(1).unwrap(), &false);
    let res = s.client.try_finalize_arbitration(&arb_id);
    assert_contract_error(res, ContractError::ArbitrationQuorumNotMet);
}

#[test]
fn quorum_majority_for_challenger_wins() {
    let s = setup();
    let arbs = nominate(&s, 5);
    let arb_id = s.client.open_arbitration(&s.dispute_id);
    s.client.vote_on_dispute(&arb_id, &arbs.get(0).unwrap(), &false);
    s.client.vote_on_dispute(&arb_id, &arbs.get(1).unwrap(), &false);
    s.client.vote_on_dispute(&arb_id, &arbs.get(2).unwrap(), &true);
    s.client.finalize_arbitration(&arb_id);
    assert_eq!(s.client.get_arbitration(&arb_id).winner, Some(s.challenger.clone()));
}

#[test]
fn tie_at_quorum_goes_to_owner() {
    let s = setup();
    let arbs = nominate(&s, 4);
    let arb_id = s.client.open_arbitration(&s.dispute_id);
    // quorum for 4 arbitrators is 3; a 2-2 tie meets it
    s.client.vote_on_dispute(&arb_id, &arbs.get(0).unwrap(), &false);
    s.client.vote_on_dispute(&arb_id, &arbs.get(1).unwrap(), &false);
    s.client.vote_on_dispute(&arb_id, &arbs.get(2).unwrap(), &true);
    s.client.vote_on_dispute(&arb_id, &arbs.get(3).unwrap(), &true);
    s.client.finalize_arbitration(&arb_id);

    assert_eq!(s.client.get_arbitration(&arb_id).winner, Some(s.owner.clone()));
    assert_eq!(s.client.get_dispute(&s.dispute_id).winner, Some(s.owner.clone()));
}
