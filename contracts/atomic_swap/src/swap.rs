use soroban_sdk::{Address, Env, Vec};

use crate::{utils::panic_with_error, ContractError, DataKey, SwapRecord, LEDGER_BUMP};

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
