#![no_std]

mod circuit_breaker;
mod drips;
mod gas;
mod guardian;
mod reentrancy;
mod reputation;
mod task;
mod types;
mod vault;
pub mod events;

use soroban_sdk::{contract, contractimpl, Address, Env, Map};
use types::{ContractError, DataKey, RewardStream, Snapshot};

pub use guardian::{add_guardian, remove_guardian, is_guardian};
pub use task::{get_task, register_task};
pub use drips::{get_reward_stream, start_drips_stream};
pub use types::Operation;

const DEFAULT_WEIGHT_THRESHOLD: u64 = 300;

#[contract]
pub struct VeroCore;

#[contractimpl]
impl VeroCore {
    pub fn initialize(env: Env, admin: Address) -> Result<(), ContractError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .extend_ttl(100_000, 100_000);
        Ok(())
    }

    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Admin)
    }

    pub fn toggle_pause(env: Env, admin: Address) -> Result<(), ContractError> {
        admin.require_auth();
        let current = env.storage().instance().get(&DataKey::Paused).unwrap_or(false);
        env.storage().instance().set(&DataKey::Paused, &!current);
        Ok(())
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), ContractError> {
        admin.require_auth();
        env.storage().instance().set(&DataKey::Paused, &true);
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), ContractError> {
        admin.require_auth();
        env.storage().instance().set(&DataKey::Paused, &false);
        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn add_guardian(env: Env, admin: Address, guardian: Address) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        guardian::add_guardian(&env, admin, guardian);
        Ok(())
    }

    pub fn remove_guardian(env: Env, admin: Address, guardian: Address) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        guardian::remove_guardian(&env, admin, guardian);
        Ok(())
    }

    pub fn is_guardian(env: Env, guardian: Address) -> bool {
        guardian::is_guardian(&env, &guardian)
    }

    pub fn set_reputation(
        env: Env,
        admin: Address,
        guardian: Address,
        score: u64,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        reputation::set_reputation(&env, admin, guardian, score);
        Ok(())
    }

    pub fn get_reputation(env: Env, guardian: Address) -> Option<u64> {
        reputation::get_reputation(&env, &guardian)
    }

    pub fn set_weight_threshold(env: Env, admin: Address, threshold: u64) -> Result<(), ContractError> {
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::WeightThreshold, &threshold);
        Ok(())
    }

    pub fn get_weight_threshold(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::WeightThreshold)
            .unwrap_or(DEFAULT_WEIGHT_THRESHOLD)
    }

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
        let task_ids = soroban_sdk::vec![&env, task_id];
        task::register_tasks(&env, admin, task_ids)
    }

    pub fn cancel_task(
        env: Env,
        admin: Address,
        task_id: u64,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        task::cancel_task(&env, admin, task_id)
    }

    pub fn vote(env: Env, guardian: Address, task_id: u64) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        guardian.require_auth();
        reentrancy::lock(&env)?;

        if !guardian::is_guardian(&env, &guardian) {
            reentrancy::unlock(&env);
            return Err(ContractError::NotAuthorized);
        }

        let token_key = DataKey::TokenAddress;
        if !env.storage().instance().has(&token_key) {
            reentrancy::unlock(&env);
            return Err(ContractError::NotInitialized);
        }
        let threshold: i128 = env.storage().instance().get(&DataKey::LockThreshold).unwrap_or(0);
        let balance_key = DataKey::LockedBalance(guardian.clone());
        let locked_balance: i128 = env.storage().instance().get(&balance_key).unwrap_or(0);

        if locked_balance <= threshold {
            reentrancy::unlock(&env);
            return Err(ContractError::InsufficientLockedBalance);
        }

        let voted_key = DataKey::Voted(task_id, guardian.clone());
        if env.storage().instance().has(&voted_key) {
            reentrancy::unlock(&env);
            return Err(ContractError::DuplicateVote);
        }

        let weight = match reputation::calculate_voting_power(&env, &guardian) {
            Some(w) => w,
            None => {
                reentrancy::unlock(&env);
                return Err(ContractError::NoReputationScore);
            }
        };

        if weight == 0 {
            reentrancy::unlock(&env);
            return Err(ContractError::ZeroWeightVote);
        }

        let task_key = DataKey::Task(task_id);
        let mut t: types::Task = match env.storage().instance().get(&task_key) {
            Some(t) => t,
            None => {
                reentrancy::unlock(&env);
                return Err(ContractError::NotAuthorized);
            }
        };

        if t.is_cancelled {
            reentrancy::unlock(&env);
            return Err(ContractError::TaskCancelled);
        }

        t.total_weight_accrued = match t.total_weight_accrued.checked_add(weight) {
            Some(v) => v,
            None => {
                reentrancy::unlock(&env);
                return Err(ContractError::WeightOverflow);
            }
        };
        t.votes += 1;

        let weight_threshold: u64 = env
            .storage()
            .instance()
            .get(&DataKey::WeightThreshold)
            .unwrap_or(DEFAULT_WEIGHT_THRESHOLD);

        if t.total_weight_accrued >= weight_threshold {
            t.is_done = true;
            events::emit_task_resolved(&env, task_id, t.total_weight_accrued);

            if let Some(vault_addr) = env.storage().instance().get::<_, Address>(&DataKey::VaultAddress) {
                let vault_client = vault::VaultClient::new(&env, &vault_addr);
                if vault_client.try_release_funds(&task_id).is_err() {
                    reentrancy::unlock(&env);
                    return Err(ContractError::EscrowUnavailable);
                }
            }
        }

        let mut all_votes: soroban_sdk::Vec<(u64, Address)> = env
            .storage()
            .instance()
            .get(&DataKey::AllVotes)
            .unwrap_or(soroban_sdk::Vec::new(&env));
        all_votes.push_back((task_id, guardian.clone()));
        env.storage().instance().set(&DataKey::AllVotes, &all_votes);

        env.storage().instance().set(&voted_key, &true);
        env.storage().instance().set(&task_key, &t);

        events::emit_weighted_vote(&env, task_id, &guardian, weight);

        reentrancy::unlock(&env);
        Ok(())
    }

    pub fn get_task(env: Env, task_id: u64) -> Option<types::Task> {
        task::get_task(&env, task_id)
    }

    pub fn start_reward_stream(
        env: Env,
        admin: Address,
        drips_address: Address,
        contributor: Address,
        task_id: u64,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env)?;
        admin.require_auth();

        let result = drips::start_drips_stream(&env, drips_address, contributor.clone(), task_id);

        match &result {
            Ok(()) => events::emit_reward_stream_started(&env, task_id, &contributor),
            Err(_) => events::emit_reward_stream_failed(&env, task_id, &contributor),
        }

        result
    }

    pub fn get_reward_stream(env: Env, task_id: u64) -> Option<RewardStream> {
        drips::get_reward_stream(&env, task_id)
    }

    // ─── Circuit breaker ───────────────────────────────────────────

    pub fn record_failure(env: Env) {
        circuit_breaker::record_failure(&env);
    }

    pub fn reset_circuit_breaker(env: Env, admin: Address) {
        circuit_breaker::reset(&env, admin);
    }

    // ─── Gas cost estimation ───────────────────────────────────────────

    /// Returns the estimated instruction-unit cost for a given [`Operation`].
    ///
    /// This is a pure view function — it performs no storage reads or writes,
    /// no authentication, and no cross-contract calls. Guardians and tooling
    /// can call this before submitting a transaction to set an appropriate
    /// resource fee and avoid "out of gas" failures.
    ///
    /// # Arguments
    /// * `op` — The [`Operation`] variant whose cost estimate is requested.
    ///
    /// # Returns
    /// A `u64` representing the conservative upper-bound instruction-unit cost,
    /// calibrated against Soroban Protocol 21 metering constants.
    pub fn get_estimated_cost(_env: Env, op: types::Operation) -> u64 {
        gas::get_estimated_cost(op)
    }

    // ─── Contract upgrade ──────────────────────────────────────────

    pub fn upgrade_contract(env: Env, admin: Address, new_wasm_hash: soroban_sdk::BytesN<32>) {
        admin.require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    // ─── Snapshot ──────────────────────────────────────────────────

    pub fn get_snapshot(env: Env) -> Snapshot {
        let paused = env.storage().instance().get(&DataKey::Paused).unwrap_or(false);
        let failure_count = env.storage().instance().get(&DataKey::FailureCount).unwrap_or(0);
        let weight_threshold = env.storage().instance().get(&DataKey::WeightThreshold).unwrap_or(DEFAULT_WEIGHT_THRESHOLD);
        let admin = env.storage().instance().get(&DataKey::Admin);
        let vault_address = env.storage().instance().get(&DataKey::VaultAddress);
        let drips_address = env.storage().instance().get(&DataKey::DripsAddress);

        let mut guardians = Map::new(&env);
        let all_guardians = guardian::get_all_guardians(&env);
        for g in all_guardians.iter() {
            guardians.set(g.clone(), guardian::is_guardian(&env, &g));
        }

        let mut reputations = Map::new(&env);
        for g in all_guardians.iter() {
            if let Some(score) = reputation::get_reputation(&env, &g) {
                reputations.set(g.clone(), score);
            }
        }

        let mut tasks = Map::new(&env);
        let all_tasks = task::get_all_tasks(&env);
        for t in all_tasks.iter() {
            if let Some(task) = task::get_task(&env, t) {
                tasks.set(t, task);
            }
        }

        let mut votes = Map::new(&env);
        let all_votes: soroban_sdk::Vec<(u64, Address)> = env
            .storage()
            .instance()
            .get(&DataKey::AllVotes)
            .unwrap_or(soroban_sdk::Vec::new(&env));
        for v in all_votes.iter() {
            votes.set(v, true);
        }

        let mut reward_streams = Map::new(&env);
        let all_streams = drips::get_all_reward_streams(&env);
        for s in all_streams.iter() {
            if let Some(stream) = drips::get_reward_stream(&env, s) {
                reward_streams.set(s, stream);
            }
        }

        Snapshot {
            paused,
            failure_count,
            weight_threshold,
            admin,
            vault_address,
            drips_address,
            guardians,
            reputations,
            tasks,
            votes,
            reward_streams,
        }
    }
}
