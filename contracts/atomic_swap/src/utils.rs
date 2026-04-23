use soroban_sdk::{Env, Error};

use crate::{ContractError, DataKey};

pub fn panic_with_error(env: &Env, error: ContractError) -> ! {
    env.panic_with_error(Error::from_contract_error(error as u32));
}

#[allow(dead_code)]
pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Paused)
        .unwrap_or(false)
}
