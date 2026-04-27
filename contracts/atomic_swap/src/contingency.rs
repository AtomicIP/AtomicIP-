use soroban_sdk::{Address, Env, Error, Vec};
use crate::{ContractError, DataKey, SwapRecord, SwapStatus, LEDGER_BUMP};
use crate::swap;

/// Complete a swap with contingency condition verification (seller-only).
pub fn complete_swap_contingent(
    env: &Env,
    swap_id: u64,
    condition_proof: Vec<u8>,
) -> Result<(), ContractError> {
    let swap = swap::get_swap(env, swap_id);

    swap.seller.require_auth();

    if swap.status != SwapStatus::Accepted {
        return Err(ContractError::SwapNotAccepted);
    }

    if swap.contingency_condition.is_none() {
        return Err(ContractError::ContingencyConditionNotMet);
    }

    let condition = swap.contingency_condition.unwrap();
    if condition != condition_proof {
        return Err(ContractError::ContingencyConditionNotMet);
    }

    let mut updated_swap = swap.clone();
    updated_swap.status = SwapStatus::Completed;
    swap::save_swap(env, swap_id, &updated_swap);

    Ok(())
}
