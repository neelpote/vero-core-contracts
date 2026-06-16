// Event emission helpers for contract state transitions.

use soroban_sdk::{symbol_short, Address, Env};

pub fn emit_task_resolved(env: &Env, task_id: u64, weight: u64) {
    env.events()
        .publish((symbol_short!("resolved"),), (task_id, weight));
}

pub fn emit_weighted_vote(env: &Env, task_id: u64, guardian: &Address, weight: u64) {
    env.events()
        .publish((symbol_short!("wt_vote"),), (task_id, guardian.clone(), weight));
}

pub fn emit_pause_toggled(env: &Env, paused: bool) {
    env.events()
        .publish((symbol_short!("paused"),), paused);
}

/// Emits an event when a reward stream is started for a contributor.
pub fn emit_reward_stream_started(env: &Env, task_id: u64, contributor: &Address) {
    env.events()
        .publish((symbol_short!("rw_start"),), (task_id, contributor.clone()));
}

/// Emits an event when a cross-contract Drips call fails.
pub fn emit_reward_stream_failed(env: &Env, task_id: u64, contributor: &Address) {
    env.events()
        .publish((symbol_short!("rw_fail"),), (task_id, contributor.clone()));
}

/// Emits an event when a guardian casts a weighted vote.
pub fn emit_weighted_vote(env: &Env, task_id: u64, guardian: &Address, weight: u64) {
    env.events()
        .publish((symbol_short!("wt_vote"),), (task_id, guardian.clone(), weight));
}

/// Emits an event when a task reaches consensus.
pub fn emit_task_resolved(env: &Env, task_id: u64, total_weight: u64) {
    env.events()
        .publish((symbol_short!("resolved"),), (task_id, total_weight));
}

/// Emits an event when the circuit breaker trips and pauses the contract.
///
/// Event topic: `"cb_trip"` (circuit_breaker_triggered)
/// Event data: `failure_count`
pub fn emit_circuit_breaker_triggered(env: &Env, failure_count: u32) {
    env.events()
        .publish((symbol_short!("cb_trip"),), (failure_count,));
}
