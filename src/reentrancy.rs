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

/// A RAII guard for the reentrancy lock.
pub struct ReentrancyGuard<'a> {
    env: &'a Env,
}

impl<'a> ReentrancyGuard<'a> {
    pub fn new(env: &'a Env) -> Result<Self, ContractError> {
        lock(env)?;
        Ok(ReentrancyGuard { env })
    }
}

impl<'a> Drop for ReentrancyGuard<'a> {
    fn drop(&mut self) {
        unlock(self.env);
    }
}

/// A macro to enforce reentrancy protection on a function block.
#[macro_export]
macro_rules! non_reentrant {
    ($env:expr) => {
        let _guard = $crate::reentrancy::ReentrancyGuard::new($env)?;
    };
}
