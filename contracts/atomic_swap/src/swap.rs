use soroban_sdk::{Address, Env, Vec};

use crate::{utils::panic_with_error, ContractError, DataKey, SwapRecord, SwapStatus, LEDGER_BUMP};

#[allow(dead_code)]
pub fn load_swap(env: &Env, swap_id: u64) -> SwapRecord {
    env.storage()
        .persistent()
        .get(&DataKey::Swap(swap_id))
        .unwrap_or_else(|| panic_with_error(env, ContractError::SwapNotFound))
}

pub fn save_swap(env: &Env, swap_id: u64, swap: &SwapRecord) {
    env.storage()
        .persistent()
        .set(&DataKey::Swap(swap_id), swap);
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::Swap(swap_id), LEDGER_BUMP, LEDGER_BUMP);

    release_insurance_reservation_if_settled(env, swap_id, swap);
}

/// #354: A swap that has reached a terminal state can no longer be claimed
/// against, so its coverage reservation must stop counting toward the pool's
/// outstanding total. Centralised here rather than at each of the ~20 status
/// transitions so no future terminal path can silently leak a reservation.
///
/// A swap already flagged `InsuranceClaimable` keeps its reservation: the claim
/// is what the reservation exists for, and `claim_insurance` releases it on
/// payout.
fn release_insurance_reservation_if_settled(env: &Env, swap_id: u64, swap: &SwapRecord) {
    let settled = matches!(
        swap.status,
        SwapStatus::Completed | SwapStatus::Cancelled | SwapStatus::RolledBack
    );
    if !settled {
        return;
    }

    if env
        .storage()
        .persistent()
        .has(&DataKey::InsuranceClaimable(swap_id))
    {
        return;
    }

    let reserved: i128 = match env
        .storage()
        .persistent()
        .get(&DataKey::InsuranceReserved(swap_id))
    {
        Some(amount) => amount,
        None => return,
    };

    env.storage()
        .persistent()
        .remove(&DataKey::InsuranceReserved(swap_id));

    let total_key = DataKey::InsuranceReservedTotal(swap.token.clone());
    let total: i128 = env.storage().persistent().get(&total_key).unwrap_or(0);
    // Saturate at zero rather than letting a double-release go negative.
    let remaining = if total > reserved { total - reserved } else { 0 };
    env.storage().persistent().set(&total_key, &remaining);
    env.storage()
        .persistent()
        .extend_ttl(&total_key, LEDGER_BUMP, LEDGER_BUMP);
}

pub fn append_swap_for_party(env: &Env, seller: &Address, buyer: &Address, swap_id: u64) {
    let mut seller_ids: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::SellerSwaps(seller.clone()))
        .unwrap_or(Vec::new(env));
    seller_ids.push_back(swap_id);
    env.storage()
        .persistent()
        .set(&DataKey::SellerSwaps(seller.clone()), &seller_ids);
    env.storage().persistent().extend_ttl(
        &DataKey::SellerSwaps(seller.clone()),
        LEDGER_BUMP,
        LEDGER_BUMP,
    );

    let mut buyer_ids: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::BuyerSwaps(buyer.clone()))
        .unwrap_or(Vec::new(env));
    buyer_ids.push_back(swap_id);
    env.storage()
        .persistent()
        .set(&DataKey::BuyerSwaps(buyer.clone()), &buyer_ids);
    env.storage().persistent().extend_ttl(
        &DataKey::BuyerSwaps(buyer.clone()),
        LEDGER_BUMP,
        LEDGER_BUMP,
    );
}
