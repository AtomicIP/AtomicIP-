use soroban_sdk::{Address, BytesN, Env, Vec};
use crate::{DataKey, LEDGER_BUMP};

/// Raise a swap dispute with evidence (buyer or seller).
pub fn raise_swap_dispute(
    env: &Env,
    swap_id: u64,
    submitter: Address,
    evidence_hash: BytesN<32>,
) {
    let mut evidence_list: Vec<BytesN<32>> = env
        .storage()
        .persistent()
        .get(&DataKey::SwapDisputeEvidence(swap_id))
        .unwrap_or(Vec::new(env));

    evidence_list.push_back(evidence_hash);

    env.storage()
        .persistent()
        .set(&DataKey::SwapDisputeEvidence(swap_id), &evidence_list);
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::SwapDisputeEvidence(swap_id), LEDGER_BUMP, LEDGER_BUMP);
}

/// Get all dispute evidence for a swap.
pub fn get_dispute_evidence_list(env: &Env, swap_id: u64) -> Vec<BytesN<32>> {
    env.storage()
        .persistent()
        .get(&DataKey::SwapDisputeEvidence(swap_id))
        .unwrap_or(Vec::new(env))
}
