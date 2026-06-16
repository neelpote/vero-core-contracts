use soroban_sdk::{Address, Env};

use crate::types::DataKey;

/// Sets the reputation score for a guardian. Only callable by admin.
///
/// # Arguments
/// * `env` - The contract environment.
/// * `admin` - The admin address (must pass `require_auth`).
/// * `guardian` - The guardian whose reputation is being set.
/// * `score` - The u64 reputation score to assign.
pub fn set_reputation(env: &Env, admin: Address, guardian: Address, score: u64) {
    admin.require_auth();

    let key = DataKey::Reputation(guardian);
    env.storage().instance().set(&key, &score);
}

/// Retrieves the raw reputation score for a guardian, if one exists.
pub fn get_reputation(env: &Env, guardian: &Address) -> Option<u64> {
    let key = DataKey::Reputation(guardian.clone());
    env.storage().instance().get(&key)
}

/// Calculates the voting power for a given guardian based on their
/// reputation score. The voting power is a direct mapping of the
/// reputation score — higher reputation yields proportionally more
/// influence in the weighted consensus.
///
/// Returns `None` if no reputation score is registered.
///
/// # Voting power tiers (example policy):
/// - Score 0: no voting power (rejected at vote time)
/// - Score 1–99: lightweight contributor
/// - Score 100–499: established contributor
/// - Score 500+: core contributor / high-trust guardian
///
/// The raw score IS the weight — tier labels are informational only.
pub fn calculate_voting_power(env: &Env, guardian: &Address) -> Option<u64> {
    get_reputation(env, guardian)
}
