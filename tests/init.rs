#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};
use vero_core_contracts::VeroCoreClient;

#[test]
fn test_registry_starts_clean() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroCore);
    let client = VeroCoreClient::new(&env, &contract_id);

    // No tasks registered yet
    assert!(client.get_task(&1u64).is_none());

    // No reward streams exist
    assert!(client.get_reward_stream(&1u64).is_none());

    // Default weight threshold is 300
    assert_eq!(client.get_weight_threshold(), 300);

    // A fresh address is not a guardian
    let stranger = Address::generate(&env);
    assert_eq!(client.calculate_voting_power(&stranger), None);
    assert_eq!(client.get_reputation(&stranger), None);
}

#[test]
fn test_reinitialize_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroCore);
    let client = VeroCoreClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token.address();

    // First call succeeds
    client.initialize(&token_addr, &100i128);

    // Second call must revert with AlreadyInitialized
    let result = client.try_initialize(&token_addr, &100i128);
    assert!(result.is_err(), "second initialize() must revert");
}
