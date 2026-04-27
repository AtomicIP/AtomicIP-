use soroban_sdk::{Address, Env};
use crate::{DataKey, LEDGER_BUMP};

/// Get user reputation (completed_swaps, rating).
pub fn get_user_reputation(env: &Env, user: Address) -> (u32, u32) {
    match env
        .storage()
        .persistent()
        .get::<DataKey, (u32, u32)>(&DataKey::UserReputation(user.clone()))
    {
        Some((completed, rating)) => (completed, rating),
        None => (0, 0),
    }
}

/// Update user reputation on swap completion.
pub fn update_reputation_on_completion(env: &Env, seller: &Address, buyer: &Address) {
    let (seller_completed, seller_rating) = get_user_reputation(env, seller.clone());
    let (buyer_completed, buyer_rating) = get_user_reputation(env, buyer.clone());

    env.storage()
        .persistent()
        .set(&DataKey::UserReputation(seller.clone()), &(seller_completed + 1, seller_rating));
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::UserReputation(seller.clone()), LEDGER_BUMP, LEDGER_BUMP);

    env.storage()
        .persistent()
        .set(&DataKey::UserReputation(buyer.clone()), &(buyer_completed + 1, buyer_rating));
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::UserReputation(buyer.clone()), LEDGER_BUMP, LEDGER_BUMP);
}
