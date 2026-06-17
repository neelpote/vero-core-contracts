use soroban_sdk::{Address, Env, Vec};

use crate::reentrancy;
use crate::types::{ContractError, DataKey, Task};

const MAX_REGISTER_TASK_BATCH_SIZE: u32 = 32;

pub fn register_tasks(env: &Env, admin: Address, task_ids: Vec<u64>) -> Result<(), ContractError> {
    if task_ids.len() > MAX_REGISTER_TASK_BATCH_SIZE {
        return Err(ContractError::BatchTooLarge);
    }

    admin.require_auth();
    reentrancy::lock(env)?;

    for task_id in task_ids.into_iter() {
        let key = DataKey::Task(task_id);
        if env.storage().instance().has(&key) {
            reentrancy::unlock(env);
            return Err(ContractError::NotAuthorized);
        }

    let mut all_tasks: Vec<u64> = env
        .storage()
        .instance()
        .get(&DataKey::AllTasks)
        .unwrap_or(Vec::new(env));
    all_tasks.push_back(task_id);
    env.storage().instance().set(&DataKey::AllTasks, &all_tasks);

    let task = Task {
        id: task_id,
        votes: 0,
        is_done: false,
        total_weight_accrued: 0,
        is_cancelled: false,
    };
    env.storage().instance().set(&key, &task);

    reentrancy::unlock(env);
    Ok(())
}

pub fn get_task(env: &Env, task_id: u64) -> Option<Task> {
    env.storage()
        .instance()
        .get(&DataKey::Task(task_id))
}

pub fn get_all_tasks(env: &Env) -> Vec<u64> {
    env.storage()
        .instance()
        .get(&DataKey::AllTasks)
        .unwrap_or(Vec::new(env))
}
