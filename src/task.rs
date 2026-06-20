use soroban_sdk::{Address, Env, Vec};

use crate::storage;
use crate::types::{ContractError, DataKey, Task};

const MAX_REGISTER_TASK_BATCH_SIZE: u32 = 32;

pub fn register_tasks(env: &Env, admin: Address, task_ids: Vec<u64>) -> Result<(), ContractError> {
    if task_ids.len() > MAX_REGISTER_TASK_BATCH_SIZE {
        return Err(ContractError::BatchTooLarge);
    }

    admin.require_auth();
    crate::non_reentrant!(env);

    let mut all_tasks: Vec<u64> = env
        .storage()
        .instance()
        .get(&DataKey::AllTasks)
        .unwrap_or(Vec::new(env));

    for task_id in task_ids.iter() {
        if storage::has_active_task(env, task_id) || storage::get_archived_task(env, task_id).is_some() {
            return Err(ContractError::NotAuthorized);
        }

        let task = Task {
            id: task_id,
            votes: 0,
            is_done: false,
            resolved_at: 0,
            total_weight_accrued: 0,
            is_cancelled: false,
        };
        storage::set_active_task(env, &task);
    }

    all_tasks.append(&task_ids);
    env.storage().instance().set(&DataKey::AllTasks, &all_tasks);

    Ok(())
}

pub fn get_task(env: &Env, task_id: u64) -> Option<Task> {
    storage::get_active_task(env, task_id)
}

pub fn get_all_tasks(env: &Env) -> Vec<u64> {
    env.storage()
        .instance()
        .get(&DataKey::AllTasks)
        .unwrap_or(Vec::new(env))
}

pub fn cancel_task(env: &Env, admin: Address, task_id: u64) -> Result<(), ContractError> {
    admin.require_auth();

    crate::non_reentrant!(env);

    let mut task: Task = match storage::get_active_task(env, task_id) {
        Some(t) => t,
        None => {
            return Err(ContractError::NotAuthorized);
        }
    };

    task.is_cancelled = true;
    storage::set_active_task(env, &task);

    crate::events::emit_task_cancelled(env, task_id);

    Ok(())
}
