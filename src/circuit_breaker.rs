use soroban_sdk::Env;

use crate::events;
use crate::types::{ContractError, DataKey};

const FAILURE_THRESHOLD: u32 = 50;

/// Returns `Err(ContractPaused)` if the contract is currently paused.
pub fn require_not_paused(env: &Env) -> Result<(), ContractError> {
    if env.storage().instance().get(&DataKey::Paused).unwrap_or(false) {
        return Err(ContractError::ContractPaused);
    }
    Ok(())
}

/// Increments the failure counter. Triggers an emergency stop if the
/// threshold is exceeded and the contract is not already paused.
/// Called from the public `record_failure` contract entry point so that
/// the storage write is never rolled back (it runs in its own invocation).
pub fn record_failure(env: &Env) {
    let count: u32 = env
        .storage()
        .instance()
        .get(&DataKey::FailureCount)
        .unwrap_or(0)
        + 1;

    env.storage().instance().set(&DataKey::FailureCount, &count);

    if count > FAILURE_THRESHOLD
        && !env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    {
        env.storage().instance().set(&DataKey::Paused, &true);
        events::emit_circuit_breaker_triggered(env, count);
    }
}

/// Resets the failure counter and unpauses the contract. Admin only.
pub fn reset(env: &Env, admin: soroban_sdk::Address) {
    admin.require_auth();
    env.storage().instance().set(&DataKey::FailureCount, &0u32);
    env.storage().instance().remove(&DataKey::Paused);
}
