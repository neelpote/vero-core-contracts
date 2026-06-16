#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};
use vero_core_contracts::VeroContractClient;

#[test]
fn test_registry_starts_clean() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);

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
