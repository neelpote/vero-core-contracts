use soroban_sdk::Env;

use crate::types::{ContractError, DataKey};

/// Acquires the mutex. Returns `Err(Locked)` if already held.
pub fn lock(env: &Env) -> Result<(), ContractError> {
    if env.storage().instance().has(&DataKey::Lock) {
        return Err(ContractError::Locked);
    }
    env.storage().instance().set(&DataKey::Lock, &true);
    Ok(())
}

/// Releases the mutex.
pub fn unlock(env: &Env) {
    env.storage().instance().remove(&DataKey::Lock);
}
