#![no_std]

mod circuit_breaker;
mod drips;
mod guardian;
mod reentrancy;
mod reputation;
mod task;
mod types;
mod vault;
mod reentrancy;
pub mod events;

use soroban_sdk::{contract, contractimpl, Address, Env};
use types::{ContractError, DataKey, RewardStream};

pub use guardian::{add_guardian, is_guardian};
pub use task::{get_task, register_task};
pub use drips::{get_reward_stream, start_drips_stream};

/// Default weight threshold: a task requires at least 300 cumulative
/// reputation weight to be resolved. This can be overridden by the
/// admin via `set_weight_threshold`.
const DEFAULT_WEIGHT_THRESHOLD: u64 = 300;

fn require_not_paused(env: &Env) -> Result<(), ContractError> {
    if env
        .storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        return Err(ContractError::ContractPaused);
    }
    Ok(())
}

#[contract]
pub struct VeroContract;

#[contractimpl]
impl VeroContract {
    pub fn initialize(
        env: Env,
        token: Address,
        threshold: i128,
    ) -> Result<(), ContractError> {
        let token_key = DataKey::TokenAddress;
        if env.storage().instance().has(&token_key) {
            return Err(ContractError::AlreadyInitialized);
        }
        env.storage().instance().set(&token_key, &token);
        env.storage().instance().set(&DataKey::LockThreshold, &threshold);
        Ok(())
    }

    pub fn lock_tokens(
        env: Env,
        guardian: Address,
        amount: i128,
    ) -> Result<(), ContractError> {
        guardian.require_auth();

        let token_key = DataKey::TokenAddress;
        if !env.storage().instance().has(&token_key) {
            return Err(ContractError::NotInitialized);
        }
        let token_address: Address = env.storage().instance().get(&token_key).unwrap();

        let client = soroban_sdk::token::Client::new(&env, &token_address);
        client.transfer(&guardian, &env.current_contract_address(), &amount);

        let balance_key = DataKey::LockedBalance(guardian.clone());
        let current_balance: i128 = env.storage().instance().get(&balance_key).unwrap_or(0);
        env.storage().instance().set(&balance_key, &(current_balance + amount));

        Ok(())
    }

    pub fn resign_guardian(
        env: Env,
        guardian: Address,
    ) -> Result<(), ContractError> {
        guardian.require_auth();

        let token_key = DataKey::TokenAddress;
        if !env.storage().instance().has(&token_key) {
            return Err(ContractError::NotInitialized);
        }

        if !guardian::is_guardian(&env, &guardian) {
            return Err(ContractError::NotGuardian);
        }

        let key = DataKey::Guardian(guardian.clone());
        env.storage().instance().set(&key, &false);

        let balance_key = DataKey::LockedBalance(guardian.clone());
        let locked_balance: i128 = env.storage().instance().get(&balance_key).unwrap_or(0);
        if locked_balance > 0 {
            let token_address: Address = env.storage().instance().get(&token_key).unwrap();
            let client = soroban_sdk::token::Client::new(&env, &token_address);
            client.transfer(&env.current_contract_address(), &guardian, &locked_balance);
            env.storage().instance().set(&balance_key, &0i128);
        }

        Ok(())
    }

    pub fn unlock_tokens(
        env: Env,
        guardian: Address,
    ) -> Result<(), ContractError> {
        guardian.require_auth();

        let token_key = DataKey::TokenAddress;
        if !env.storage().instance().has(&token_key) {
            return Err(ContractError::NotInitialized);
        }

        if guardian::is_guardian(&env, &guardian) {
            return Err(ContractError::StillGuardian);
        }

        let balance_key = DataKey::LockedBalance(guardian.clone());
        let locked_balance: i128 = env.storage().instance().get(&balance_key).unwrap_or(0);
        if locked_balance > 0 {
            let token_address: Address = env.storage().instance().get(&token_key).unwrap();
            let client = soroban_sdk::token::Client::new(&env, &token_address);
            client.transfer(&env.current_contract_address(), &guardian, &locked_balance);
            env.storage().instance().set(&balance_key, &0i128);
        }

        Ok(())
    }

    // ─── Emergency stop ────────────────────────────────────────────

    /// Toggles the global pause state. Only callable by admin.
    /// When paused, all public methods return `ContractPaused`.
    pub fn toggle_pause(env: Env, admin: Address) {
        admin.require_auth();
        let current: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        env.storage().instance().set(&DataKey::Paused, &!current);
        events::emit_pause_toggled(&env, !current);
    }

    /// Returns the current pause state.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    // ─── Guardian management ───────────────────────────────────────

    pub fn add_guardian(env: Env, admin: Address, guardian: Address) -> Result<(), ContractError> {
        require_not_paused(&env)?;
        guardian::add_guardian(&env, admin, guardian);
        Ok(())
    }

    pub fn is_guardian(env: Env, guardian: Address) -> bool {
        guardian::is_guardian(&env, &guardian)
    }

    // ─── Reputation management ─────────────────────────────────────

    /// Sets the reputation score for a guardian. Only callable by admin.
    pub fn set_reputation(
        env: Env,
        admin: Address,
        guardian: Address,
        score: u64,
    ) -> Result<(), ContractError> {
        require_not_paused(&env)?;
        reputation::set_reputation(&env, admin, guardian, score);
        Ok(())
    }

    /// Returns the raw reputation score for a guardian.
    pub fn get_reputation(env: Env, guardian: Address) -> Result<Option<u64>, ContractError> {
        require_not_paused(&env)?;
        Ok(reputation::get_reputation(&env, &guardian))
    }

    /// Calculates the voting power (weight) for a given guardian
    /// based on their reputation score.
    pub fn calculate_voting_power(
        env: Env,
        guardian: Address,
    ) -> Result<Option<u64>, ContractError> {
        require_not_paused(&env)?;
        Ok(reputation::calculate_voting_power(&env, &guardian))
    }

    /// Sets the cumulative weight threshold required to resolve a task.
    /// Only callable by admin.
    pub fn set_weight_threshold(
        env: Env,
        admin: Address,
        threshold: u64,
    ) -> Result<(), ContractError> {
        require_not_paused(&env)?;
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::WeightThreshold, &threshold);
        Ok(())
    }

    /// Returns the current weight threshold, falling back to the
    /// compiled default if none has been set.
    pub fn get_weight_threshold(env: Env) -> Result<u64, ContractError> {
        require_not_paused(&env)?;
        Ok(env
            .storage()
            .instance()
            .get(&DataKey::WeightThreshold)
            .unwrap_or(DEFAULT_WEIGHT_THRESHOLD))
    }

    /// Sets the vault address for payout release. Only callable by admin.
    pub fn set_vault_address(env: Env, admin: Address, vault: Address) {
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::VaultAddress, &vault);
    }

    // ─── Task lifecycle ────────────────────────────────────────────

    pub fn register_task(
        env: Env,
        admin: Address,
        task_id: u64,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        task::register_task(&env, admin, task_id)
    }

    /// Casts a weighted vote on a task. The guardian's reputation score
    /// determines their voting power. The vote weight is atomically
    /// added to the task's `total_weight_accrued`. Once the cumulative
    /// weight meets or exceeds the threshold, the task is resolved.
    ///
    /// # Errors
    /// * `ContractPaused`    — contract is currently paused.
    /// * `NotAuthorized`     — caller is not a registered guardian, or task not found.
    /// * `DuplicateVote`     — guardian already voted on this task.
    /// * `NoReputationScore` — guardian has no reputation score assigned.
    /// * `ZeroWeightVote`    — guardian's reputation score is zero.
    /// * `WeightOverflow`    — adding the weight would overflow u64.
    pub fn vote(env: Env, guardian: Address, task_id: u64) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        guardian.require_auth();
        reentrancy::lock(&env)?;

        reentrancy::lock(&env)?;

        // 1. Verify guardian status
        if !guardian::is_guardian(&env, &guardian) {
            reentrancy::unlock(&env);
            return Err(ContractError::NotAuthorized);
        }

        let token_key = DataKey::TokenAddress;
        if !env.storage().instance().has(&token_key) {
            return Err(ContractError::NotInitialized);
        }
        let threshold: i128 = env.storage().instance().get(&DataKey::LockThreshold).unwrap_or(0);
        let balance_key = DataKey::LockedBalance(guardian.clone());
        let locked_balance: i128 = env.storage().instance().get(&balance_key).unwrap_or(0);

        if locked_balance <= threshold {
            return Err(ContractError::InsufficientLockedBalance);
        }

        let voted_key = DataKey::Voted(task_id, guardian.clone());
        if env.storage().instance().has(&voted_key) {
            reentrancy::unlock(&env);
            return Err(ContractError::DuplicateVote);
        }

        // 3. Fetch voting power from reputation — single storage read
        let weight = match reputation::calculate_voting_power(&env, &guardian) {
            Some(w) => w,
            None => {
                return Err(ContractError::NoReputationScore);
            }
        };

        if weight == 0 {
            reentrancy::unlock(&env);
            return Err(ContractError::ZeroWeightVote);
        }

        // 4. Load the task — single storage read
        let task_key = DataKey::Task(task_id);
        let mut t: types::Task = match env.storage().instance().get(&task_key) {
            Some(t) => t,
            None => {
                reentrancy::unlock(&env);
                return Err(ContractError::NotAuthorized);
            }
        };

        // 5. Atomically increment weight with overflow protection
        t.total_weight_accrued = match t.total_weight_accrued.checked_add(weight) {
            Some(v) => v,
            None => {
                return Err(ContractError::WeightOverflow);
            }
        };
        t.votes += 1;

        // 6. Check weight threshold for consensus
        let threshold: u64 = env
            .storage()
            .instance()
            .get(&DataKey::WeightThreshold)
            .unwrap_or(DEFAULT_WEIGHT_THRESHOLD);

        if t.total_weight_accrued >= threshold {
            t.is_done = true;
            events::emit_task_resolved(&env, task_id, t.total_weight_accrued);
            
            // Release funds from escrow if configured
            if let Some(vault_addr) = env.storage().instance().get::<_, Address>(&DataKey::VaultAddress) {
                let vault_client = vault::VaultClient::new(&env, &vault_addr);
                // Call try_release_funds, which catches VM traps from the cross-contract call
                if vault_client.try_release_funds(&task_id).is_err() {
                    reentrancy::unlock(&env);
                    return Err(ContractError::EscrowUnavailable);
                }
            }
        }

        // 7. Persist vote record and updated task — two storage writes
        env.storage().instance().set(&voted_key, &true);
        env.storage().instance().set(&task_key, &t);

        events::emit_weighted_vote(&env, task_id, &guardian, weight);

        reentrancy::unlock(&env);
        Ok(())
    }

    pub fn get_task(env: Env, task_id: u64) -> Result<Option<types::Task>, ContractError> {
        require_not_paused(&env)?;
        Ok(task::get_task(&env, task_id))
    }

    /// Initiates a reward stream via the Drips protocol for a verified task.
    ///
    /// The caller (admin) must be authorized. The task must already be marked
    /// `is_done` via guardian consensus before a stream can be started.
    pub fn start_reward_stream(
        env: Env,
        admin: Address,
        drips_address: Address,
        contributor: Address,
        task_id: u64,
    ) -> Result<(), ContractError> {
        require_not_paused(&env)?;
        admin.require_auth();

        let result =
            drips::start_drips_stream(&env, drips_address, contributor.clone(), task_id);

        match &result {
            Ok(()) => {
                events::emit_reward_stream_started(&env, task_id, &contributor);
            }
            Err(_) => {
                events::emit_reward_stream_failed(&env, task_id, &contributor);
            }
        }

        result
    }

    /// Returns the reward stream record for a given task, if one exists.
    pub fn get_reward_stream(
        env: Env,
        task_id: u64,
    ) -> Result<Option<RewardStream>, ContractError> {
        require_not_paused(&env)?;
        Ok(drips::get_reward_stream(&env, task_id))
    }

    // ─── Circuit breaker ───────────────────────────────────────────

    /// Returns true if the contract is currently paused by the circuit breaker.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Reports a transaction failure to the circuit breaker.
    /// Anyone can call this after observing a failed contract invocation.
    /// Storage writes here are committed because this call succeeds (returns Ok).
    /// If the failure count exceeds the threshold, the contract is paused and
    /// a `cb_trip` event is published to alert the admin.
    pub fn record_failure(env: Env) {
        circuit_breaker::record_failure(&env);
    }

    /// Resets the failure counter and unpauses the contract. Admin only.
    pub fn reset_circuit_breaker(env: Env, admin: Address) {
        circuit_breaker::reset(&env, admin);
    }
}
