#![cfg(test)]

use soroban_sdk::{
    testutils::Address as _,
    Address, Env,
};
use vero_core_contracts::VeroCoreClient;

const LOCK_THRESHOLD: i128 = 100;

fn mint_and_lock(
    env: &Env,
    token: &Address,
    client: &VeroCoreClient,
    guardian: &Address,
    amount: i128,
) {
    lock_for_guardian(env, token, client, guardian, amount);
}


/// Deploy the contract, a native token, call initialize, and return
/// (env, admin, token_address, client).
fn setup() -> (Env, Address, Address, VeroCoreClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, vero_core_contracts::VeroCore);
    let client = VeroCoreClient::new(&env, &contract_id);

    let admin = Address::generate(&env);

    // Deploy a Stellar Asset token contract so lock_tokens / resign_guardian work.
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_addr = token.address();

    // Initialize with lock threshold = 100
    client.initialize(&token_addr, &100i128);

    (env, admin, token_addr, client)
}

/// Register a guardian and assign a reputation score in one step.
fn add_guardian_with_rep(
    env: &Env,
    token: &Address,
    client: &VeroCoreClient,
    admin: &Address,
    score: u64,
) -> Address {
    let g = Address::generate(env);
    client.add_guardian(admin, &g);
    client.set_reputation(admin, &g, &score);
    lock_for_guardian(env, token, client, &g, 101);
    g
}

/// Mint `amount` tokens to `guardian` and lock them in the contract.
fn lock_for_guardian(
    env: &Env,
    token: &Address,
    client: &VeroCoreClient,
    guardian: &Address,
    amount: i128,
) {
    let asset_client = soroban_sdk::token::StellarAssetClient::new(env, token);
    asset_client.mint(guardian, &amount);
    client.lock_tokens(guardian, &amount);
}

#[test]
fn test_add_guardian_and_register_task() {
    let (env, admin, _token, client) = setup();
    let guardian = Address::generate(&env);

    client.add_guardian(&admin, &guardian);
    client.register_task(&admin, &1u64);

    let task = client.get_task(&1u64).unwrap();
    assert_eq!(task.id, 1);
    assert_eq!(task.votes, 0);
    assert_eq!(task.total_weight_accrued, 0);
    assert_eq!(task.resolved_at, 0);
    assert!(!task.is_done);
}

#[test]
fn test_set_and_get_reputation() {
    let (env, admin, _token, client) = setup();
    let guardian = Address::generate(&env);

    client.add_guardian(&admin, &guardian);
    client.set_reputation(&admin, &guardian, &500u64);

    assert_eq!(client.get_reputation(&guardian), Some(500));
    assert_eq!(client.calculate_voting_power(&guardian), Some(500));
}

#[test]
fn test_calculate_voting_power_returns_score() {
    let (env, admin, _token, client) = setup();
    let guardian = Address::generate(&env);

    client.add_guardian(&admin, &guardian);
    client.set_reputation(&admin, &guardian, &150u64);

    let power = client.calculate_voting_power(&guardian);
    assert_eq!(power, Some(150));
}

#[test]
fn test_calculate_voting_power_none_for_unset() {
    let (env, _admin, _token, client) = setup();
    let stranger = Address::generate(&env);

    let power = client.calculate_voting_power(&stranger);
    assert_eq!(power, None);
}

// ─── Weighted consensus ─────────────────────────────────────────────

#[test]
fn test_single_high_rep_guardian_resolves_task() {
    let (env, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let g = add_guardian_with_rep(&env, &token, &client, &admin, 300);
    client.register_task(&admin, &1u64);
    lock_for_guardian(&env, &token, &client, &g, 101);
    client.vote(&g, &1u64);

    let task = client.get_task(&1u64).unwrap();
    assert_eq!(task.votes, 1);
    assert_eq!(task.total_weight_accrued, 300);
    assert!(task.is_done);
}

#[test]
fn test_multiple_low_rep_guardians_accumulate_weight() {
    let (env, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let g1 = add_guardian_with_rep(&env, &token, &client, &admin, 100);
    let g2 = add_guardian_with_rep(&env, &token, &client, &admin, 100);
    let g3 = add_guardian_with_rep(&env, &token, &client, &admin, 100);

    client.register_task(&admin, &42u64);

    client.vote(&g1, &42u64);
    client.vote(&g2, &42u64);
    client.vote(&g3, &42u64);

    let task = client.get_task(&42u64).unwrap();
    assert_eq!(task.votes, 3);
    assert!(task.is_done);
}

#[test]
fn test_weight_vs_count_logic() {
    let (env, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let g1 = add_guardian_with_rep(&env, &token, &client, &admin, 200);
    let g2 = add_guardian_with_rep(&env, &token, &client, &admin, 150);

    client.register_task(&admin, &20u64);

    lock_for_guardian(&env, &token, &client, &g1, 101);
    lock_for_guardian(&env, &token, &client, &g2, 101);

    client.vote(&g1, &20u64);
    client.vote(&g2, &20u64);

    let task = client.get_task(&20u64).unwrap();
    assert_eq!(task.votes, 2);
    assert_eq!(task.total_weight_accrued, 350);
    assert!(task.is_done);
}

#[test]
fn test_many_low_rep_guardians_cannot_resolve_without_enough_weight() {
    let (env, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let guardians: [Address; 5] = core::array::from_fn(|_| {
        add_guardian_with_rep(&env, &token, &client, &admin, 50)
    });

    client.register_task(&admin, &10u64);

    for g in &guardians {
        lock_for_guardian(&env, &token, &client, g, 101);
        client.vote(g, &10u64);
    }

    let task = client.get_task(&10u64).unwrap();
    assert_eq!(task.votes, 5);
    assert_eq!(task.total_weight_accrued, 250);
    assert!(!task.is_done);
}

#[test]
fn test_task_resolved_includes_final_weight() {
    let (env, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &100u64);

    let g1 = add_guardian_with_rep(&env, &token, &client, &admin, 42);
    let g2 = add_guardian_with_rep(&env, &token, &client, &admin, 73);

    client.register_task(&admin, &40u64);

    lock_for_guardian(&env, &token, &client, &g1, 101);
    lock_for_guardian(&env, &token, &client, &g2, 101);

    client.vote(&g1, &40u64);
    client.vote(&g2, &40u64);

    let task = client.get_task(&40u64).unwrap();
    assert_eq!(task.total_weight_accrued, 115);
    assert!(task.is_done);
}
#[test]
fn test_vote_rejects_missing_or_zero_reputation() {
    let (env, admin, token, client) = setup();
    let no_rep = Address::generate(&env);
    let zero_rep = Address::generate(&env);

    client.add_guardian(&admin, &no_rep);
    mint_and_lock(&env, &token, &client, &no_rep, LOCK_THRESHOLD + 1);

    client.add_guardian(&admin, &zero_rep);
    client.set_reputation(&admin, &zero_rep, &0u64);
    mint_and_lock(&env, &token, &client, &zero_rep, LOCK_THRESHOLD + 1);

    client.register_task(&admin, &11u64);

    assert!(client.try_vote(&no_rep, &11u64).is_err());
    assert!(client.try_vote(&zero_rep, &11u64).is_err());
}

#[test]
fn test_custom_weight_threshold() {
    let (_env, admin, _token, client) = setup();

    assert_eq!(client.get_weight_threshold(), 300);

    client.set_weight_threshold(&admin, &1000u64);
    assert_eq!(client.get_weight_threshold(), 1000);
}

#[test]
fn test_vote_rejected_without_reputation() {
    let (env, admin, token, client) = setup();
    let g = Address::generate(&env);
    client.add_guardian(&admin, &g);
    client.register_task(&admin, &7u64);
    lock_for_guardian(&env, &token, &client, &g, LOCK_THRESHOLD + 1);

    // No reputation set → should fail
    let result = client.try_vote(&g, &7u64);
    assert!(result.is_err());
}
#[test]
fn test_token_locking_rules() {
    let (env, admin, token, client) = setup();
    let guardian = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.add_guardian(&admin, &guardian);
    client.set_reputation(&admin, &guardian, &100);
    client.register_task(&admin, &12u64);
    client.register_task(&admin, &99u64);

    // Vote fails if balance = 0
    assert!(client.try_vote(&guardian, &12u64).is_err());

    // Vote fails if balance <= threshold (100)
    mint_and_lock(&env, &token, &client, &guardian, LOCK_THRESHOLD);
    assert!(client.try_vote(&guardian, &12u64).is_err());

    // Stranger cannot vote
    let result = client.try_vote(&stranger, &99u64);
    assert!(result.is_err());
}

#[test]
fn test_vote_on_nonexistent_task_rejected() {
    let (env, admin, token, client) = setup();
    let g = add_guardian_with_rep(&env, &token, &client, &admin, 100);

    let result = client.try_vote(&g, &999u64);
    assert!(result.is_err());
}

// ─── Reputation update ──────────────────────────────────────────────

#[test]
fn test_reputation_can_be_updated() {
    let (env, admin, _token, client) = setup();
    let g = Address::generate(&env);

    client.add_guardian(&admin, &g);
    client.set_reputation(&admin, &g, &100u64);
    assert_eq!(client.get_reputation(&g), Some(100));

    client.set_reputation(&admin, &g, &500u64);
    assert_eq!(client.get_reputation(&g), Some(500));
    assert_eq!(client.calculate_voting_power(&g), Some(500));
}

// ─── Drips integration ─────────────────────────────────────────────
#[test]
fn test_reward_stream_rejected_for_unverified_task() {
    let (env, admin, _token, client) = setup();
    let contributor = Address::generate(&env);
    let drips_addr = Address::generate(&env);

    client.register_task(&admin, &10u64);

    let result = client.try_start_reward_stream(&admin, &drips_addr, &contributor, &10u64);
    assert!(result.is_err());
}


#[test]
fn test_reward_stream_rejected_until_task_verified() {
    let (env, admin, _token, client) = setup();
    let contributor = Address::generate(&env);
    let drips_addr = Address::generate(&env);

    let result = client.try_start_reward_stream(&admin, &drips_addr, &contributor, &999u64);
    assert!(result.is_err());
}

#[test]
fn test_reward_stream_duplicate_rejected() {
    let (env, admin, token, client) = setup();
    let contributor = Address::generate(&env);

    let g1 = add_guardian_with_rep(&env, &token, &client, &admin, 100);
    let g2 = add_guardian_with_rep(&env, &token, &client, &admin, 100);
    let g3 = add_guardian_with_rep(&env, &token, &client, &admin, 100);
    client.register_task(&admin, &50u64);

    client.vote(&g1, &50u64);
    client.vote(&g2, &50u64);
    client.vote(&g3, &50u64);

    let drips_contract_id = env.register_contract(None, MockDripsContract);

    client.start_reward_stream(&admin, &drips_contract_id, &contributor, &50u64);

    let result = client.try_start_reward_stream(&admin, &drips_contract_id, &contributor, &50u64);
    assert!(result.is_err());
}

#[test]
fn test_reward_stream_stored_after_success() {
    let (env, admin, token, client) = setup();
    let contributor = Address::generate(&env);
    let _guardian = add_guardian_with_rep(&env, &token, &client, &admin, 300);

    let g1 = add_guardian_with_rep(&env, &token, &client, &admin, 100);
    let g2 = add_guardian_with_rep(&env, &token, &client, &admin, 100);
    let g3 = add_guardian_with_rep(&env, &token, &client, &admin, 100);
    client.register_task(&admin, &77u64);

    client.vote(&g1, &77u64);
    client.vote(&g2, &77u64);
    client.vote(&g3, &77u64);

    let drips_contract_id = env.register_contract(None, MockDripsContract);
    client.start_reward_stream(&admin, &drips_contract_id, &contributor, &77u64);

    let stream = client.get_reward_stream(&77u64).unwrap();
    assert_eq!(stream.task_id, 77);
    assert_eq!(stream.contributor, contributor);
    assert!(stream.active);
}

// ─── Token locking ─────────────────────────────────────────────────

#[test]
fn test_voting_fails_if_tokens_not_locked() {
    let (env, admin, _token, client) = setup();
    let g = Address::generate(&env);

    client.add_guardian(&admin, &g);
    client.set_reputation(&admin, &g, &100u64);
    client.register_task(&admin, &100u64);

    let result = client.try_vote(&g, &100u64);
    assert!(result.is_err());
}

#[test]
fn test_admin_pause_and_circuit_breaker() {
    let (env, admin, token, client) = setup();
    let g = Address::generate(&env);

    client.add_guardian(&admin, &g);
    client.set_reputation(&admin, &g, &100u64);
    client.register_task(&admin, &100u64);

    // Lock exactly threshold (100) — must be > threshold to vote
    lock_for_guardian(&env, &token, &client, &g, 100);
    assert!(client.try_vote(&g, &100u64).is_err());

    // Lock 1 more (total 101 > 100)
    lock_for_guardian(&env, &token, &client, &g, 1);
    client.vote(&g, &100u64);
    assert_eq!(client.get_task(&100u64).unwrap().votes, 1);
}

#[test]
fn test_resign_guardian_refunds_tokens() {
    let (env, admin, token, client) = setup();
    let g = Address::generate(&env);

    client.add_guardian(&admin, &g);
    lock_for_guardian(&env, &token, &client, &g, 200);

    client.resign_guardian(&g);

    assert!(!client.is_guardian(&g));

    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&g), 200);
}

#[test]
fn test_unlock_fails_while_guardian() {
    let (env, admin, token, client) = setup();
    let g = Address::generate(&env);

    client.add_guardian(&admin, &g);
    lock_for_guardian(&env, &token, &client, &g, 200);

    let result = client.try_unlock_tokens(&g);
    assert!(result.is_err());
}

#[test]
fn test_unlock_succeeds_for_non_guardian() {
    let (env, _admin, token, client) = setup();
    let non_guardian = Address::generate(&env);

    lock_for_guardian(&env, &token, &client, &non_guardian, 150);
    client.unlock_tokens(&non_guardian);

    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&non_guardian), 150);
}

// ─── Re-entrancy protection ─────────────────────────────────────────

#[test]
fn test_lock_released_after_successful_vote() {
    let (env, admin, token, client) = setup();
    let g1 = add_guardian_with_rep(&env, &token, &client, &admin, 100);
    let g2 = add_guardian_with_rep(&env, &token, &client, &admin, 100);

    client.register_task(&admin, &202u64);

    lock_for_guardian(&env, &token, &client, &g1, 101);
    lock_for_guardian(&env, &token, &client, &g2, 101);

    client.vote(&g1, &202u64);
    client.vote(&g2, &202u64);

    assert_eq!(client.get_task(&202u64).unwrap().votes, 2);
}

#[test]
fn test_lock_released_after_successful_register_task() {
    let (_env, admin, _token, client) = setup();

    client.register_task(&admin, &300u64);
    client.register_task(&admin, &301u64);

    assert!(client.get_task(&300u64).is_some());
    assert!(client.get_task(&301u64).is_some());
}

#[test]
fn test_lock_released_after_failed_vote() {
    let (env, admin, token, client) = setup();
    let g = add_guardian_with_rep(&env, &token, &client, &admin, 100);
    let stranger = Address::generate(&env);

    client.register_task(&admin, &303u64);

    let _ = client.try_vote(&stranger, &303u64);

    lock_for_guardian(&env, &token, &client, &g, 101);
    client.vote(&g, &303u64);
    assert_eq!(client.get_task(&303u64).unwrap().votes, 1);
}

// ─── Emergency stop (pause/unpause) ────────────────────────────────

#[test]
fn test_admin_can_toggle_pause() {
    let (_env, admin, _token, client) = setup();

    assert!(!client.is_paused());

    assert!(!client.is_paused());
    client.toggle_pause(&admin);
    assert!(client.is_paused());

    client.toggle_pause(&admin);
    assert!(!client.is_paused());
}

#[test]
fn test_admin_can_pause_and_unpause() {
    let (_env, admin, _token, client) = setup();

    client.pause(&admin);
    assert!(client.is_paused());

    client.unpause(&admin);
    assert!(!client.is_paused());
}

#[test]
fn test_contract_paused_error_on_register_task() {
    let (_env, admin, _token, client) = setup();

    for _ in 0..51 {
        client.record_failure();
    }
    assert!(client.is_paused());

    let result = client.try_register_task(&admin, &1u64);
    assert!(result.is_err());
}

#[test]
fn test_circuit_breaker_paused_contract_rejects_register_task() {
    let (_env, admin, _token, client) = setup();
    for _ in 0..51 {
        client.record_failure();
    }
    assert!(client.is_paused());

    let result = client.try_register_task(&admin, &2u64);
    assert!(result.is_err(), "register_task should be rejected while paused");
}

#[test]
fn test_contract_paused_error_on_vote() {
    let (env, admin, token, client) = setup();
    let g = add_guardian_with_rep(&env, &token, &client, &admin, 300);
    client.register_task(&admin, &1u64);
    lock_for_guardian(&env, &token, &client, &g, 101);

    client.toggle_pause(&admin);

    let result = client.try_vote(&g, &1u64);
    assert!(result.is_err());
}

#[test]
fn test_contract_paused_error_on_add_guardian() {
    let (env, admin, _token, client) = setup();
    let guardian = Address::generate(&env);

    client.toggle_pause(&admin);

    let result = client.try_add_guardian(&admin, &guardian);
    assert!(result.is_err());
}

#[test]
fn test_contract_paused_error_on_set_reputation() {
    let (env, admin, _token, client) = setup();
    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);

    client.toggle_pause(&admin);

    let result = client.try_set_reputation(&admin, &guardian, &100u64);
    assert!(result.is_err());
}

#[test]
fn test_operations_resume_after_unpause() {
    let (env, admin, token, client) = setup();
    let g = add_guardian_with_rep(&env, &token, &client, &admin, 300);

    client.toggle_pause(&admin);
    assert!(client.try_register_task(&admin, &1u64).is_err());

    client.toggle_pause(&admin);
    client.register_task(&admin, &1u64);
    lock_for_guardian(&env, &token, &client, &g, 101);
    client.vote(&g, &1u64);

    let task = client.get_task(&1u64).unwrap();
    assert!(task.is_done);
}

/// Tests for explicit pause() and unpause() methods (spec requirement).
#[test]
fn test_paused_contract_rejects_register_task() {
    let (_env, admin, _token, client) = setup();

    client.pause(&admin);
    assert!(client.is_paused());

    let result = client.try_register_task(&admin, &2u64);
    assert!(result.is_err());

    client.unpause(&admin);
    assert!(!client.is_paused());
    client.register_task(&admin, &2u64);
    assert!(client.get_task(&2u64).is_some());
}

#[test]
fn test_paused_contract_rejects_vote() {
    let (env, admin, token, client) = setup();
    client.register_task(&admin, &1u64);
    let g = add_guardian_with_rep(&env, &token, &client, &admin, 100);
    lock_for_guardian(&env, &token, &client, &g, 101);

    client.pause(&admin);
    assert!(client.is_paused());

    let result = client.try_vote(&g, &1u64);
    assert!(result.is_err());
}

// ─── Mock Drips contract ───────────────────────────────────────────

use soroban_sdk::{contract, contractimpl};

#[contract]
pub struct MockDripsContract;

#[contractimpl]
impl MockDripsContract {
    pub fn start_stream(
        _env: Env,
        _contributor: Address,
        _task_id: u64,
        _resolution_status: u32,
    ) {
        // Mock: accept silently
    }
}

// ─── Circuit breaker ───────────────────────────────────────────────

#[test]
fn test_circuit_breaker_trips_after_threshold() {
    let (_env, _admin, _token, client) = setup();
    for _ in 0..51 {
        client.record_failure();
    }
    assert!(client.is_paused());
}

#[test]
fn test_paused_contract_rejects_vote_via_circuit_breaker() {
    let (env, admin, token, client) = setup();
    client.register_task(&admin, &1u64);
    let g = add_guardian_with_rep(&env, &token, &client, &admin, 100);
    lock_for_guardian(&env, &token, &client, &g, 101);

    for _ in 0..51 {
        client.record_failure();
    }
    assert!(client.is_paused());

    let result = client.try_vote(&g, &1u64);
    assert!(result.is_err());
}

#[test]
fn test_admin_can_reset_circuit_breaker() {
    let (env, admin, token, client) = setup();
    client.register_task(&admin, &1u64);
    let g = add_guardian_with_rep(&env, &token, &client, &admin, 100);
    lock_for_guardian(&env, &token, &client, &g, 101);

    for _ in 0..51 {
        client.record_failure();
    }
    assert!(client.is_paused());

    client.reset_circuit_breaker(&admin);
    assert!(!client.is_paused());

    let result = client.try_vote(&g, &1u64);
    assert!(result.is_ok());
}

#[test]
fn debug_circuit_breaker_count() {
    let (_env, _admin, _token, client) = setup();
    // Verify count increments without tripping before threshold
    for _ in 0..50 {
        client.record_failure();
    }
    assert!(!client.is_paused(), "should not be paused at exactly threshold");

    client.record_failure(); // 51st → trips
    assert!(client.is_paused());
}

// ─── Gas cost estimation ───────────────────────────────────────────

use vero_core_contracts::Operation;

/// Every operation must return a non-zero cost. A zero would mean the
/// mapping is incomplete and guardians would under-estimate their gas.
#[test]
fn test_all_operations_return_nonzero_cost() {
    let (_env, _admin, _token, client) = setup();

    let ops = [
        Operation::RegisterTask,
        Operation::Vote,
        Operation::AddGuardian,
        Operation::SetReputation,
        Operation::LockTokens,
        Operation::UnlockTokens,
        Operation::ResignGuardian,
        Operation::SetWeightThreshold,
        Operation::StartRewardStream,
        Operation::TogglePause,
        Operation::RecordFailure,
        Operation::ResetCircuitBreaker,
        Operation::UpgradeContract,
    ];

    for op in ops {
        let cost = client.get_estimated_cost(&op);
        assert!(cost > 0, "Operation {:?} returned zero cost — mapping is incomplete", op);
    }
}

/// `Vote` is the most complex write path (5+ reads, 2 writes, conditional
/// cross-contract call). Its estimated cost must be at least as high as
/// every other write operation except `UpgradeContract` (platform fixed cost).
#[test]
fn test_vote_is_most_expensive_write_operation() {
    let (_env, _admin, _token, client) = setup();

    let vote_cost = client.get_estimated_cost(&Operation::Vote);

    let other_write_ops = [
        Operation::RegisterTask,
        Operation::AddGuardian,
        Operation::SetReputation,
        Operation::LockTokens,
        Operation::UnlockTokens,
        Operation::ResignGuardian,
        Operation::SetWeightThreshold,
        Operation::StartRewardStream,
        Operation::TogglePause,
        Operation::RecordFailure,
        Operation::ResetCircuitBreaker,
    ];

    for op in other_write_ops {
        let cost = client.get_estimated_cost(&op);
        assert!(
            vote_cost >= cost,
            "Vote ({}) should be >= {:?} ({})",
            vote_cost, op, cost
        );
    }
}

/// `UpgradeContract` carries the highest platform overhead (WASM hash write).
/// It must be the single most expensive operation overall.
#[test]
fn test_upgrade_contract_is_overall_maximum() {
    let (_env, _admin, _token, client) = setup();

    let upgrade_cost = client.get_estimated_cost(&Operation::UpgradeContract);

    let all_ops = [
        Operation::RegisterTask,
        Operation::Vote,
        Operation::AddGuardian,
        Operation::SetReputation,
        Operation::LockTokens,
        Operation::UnlockTokens,
        Operation::ResignGuardian,
        Operation::SetWeightThreshold,
        Operation::StartRewardStream,
        Operation::TogglePause,
        Operation::RecordFailure,
        Operation::ResetCircuitBreaker,
    ];

    for op in all_ops {
        let cost = client.get_estimated_cost(&op);
        assert!(
            upgrade_cost >= cost,
            "UpgradeContract ({}) should be >= {:?} ({})",
            upgrade_cost, op, cost
        );
    }
}

/// Spot-check specific constant values so any accidental regression in the
/// cost table is immediately caught.
#[test]
fn test_cost_spot_checks() {
    let (_env, _admin, _token, client) = setup();

    // Cheapest write ops — no cross-contract call
    assert_eq!(client.get_estimated_cost(&Operation::SetWeightThreshold), 650_000);
    assert_eq!(client.get_estimated_cost(&Operation::SetReputation),       700_000);
    assert_eq!(client.get_estimated_cost(&Operation::AddGuardian),         700_000);
    assert_eq!(client.get_estimated_cost(&Operation::TogglePause),         730_000);
    assert_eq!(client.get_estimated_cost(&Operation::ResetCircuitBreaker), 800_000);
    assert_eq!(client.get_estimated_cost(&Operation::RecordFailure),       880_000);
    assert_eq!(client.get_estimated_cost(&Operation::RegisterTask),      1_000_000);

    // Ops with a cross-contract call
    assert_eq!(client.get_estimated_cost(&Operation::LockTokens),        1_250_000);
    assert_eq!(client.get_estimated_cost(&Operation::StartRewardStream), 1_330_000);
    assert_eq!(client.get_estimated_cost(&Operation::UnlockTokens),      1_300_000);
    assert_eq!(client.get_estimated_cost(&Operation::ResignGuardian),    1_400_000);

    // Most complex operations
    assert_eq!(client.get_estimated_cost(&Operation::Vote),              1_960_000);
    assert_eq!(client.get_estimated_cost(&Operation::UpgradeContract),   2_500_000);
}

/// `get_estimated_cost` must be callable without any auth setup — it is a
/// pure view function with no storage access or side effects.
#[test]
fn test_estimated_cost_requires_no_auth() {
    // Intentionally do NOT call env.mock_all_auths() — we use a fresh env.
    let env = Env::default();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroCore);
    let client = VeroCoreClient::new(&env, &contract_id);

    // Should not panic even with no auth mocked and no initialize() called.
    let cost = client.get_estimated_cost(&Operation::Vote);
    assert!(cost > 0);
}

/// All cost estimates must stay above the Soroban base invocation overhead
/// (~500_000 instructions). Anything below that would be physically impossible
/// on-chain and would cause guardian transactions to consistently fail.
#[test]
fn test_all_costs_above_base_invocation_overhead() {
    let (_env, _admin, _token, client) = setup();
    const BASE_OVERHEAD: u64 = 500_000;

    let ops = [
        Operation::RegisterTask,
        Operation::Vote,
        Operation::AddGuardian,
        Operation::SetReputation,
        Operation::LockTokens,
        Operation::UnlockTokens,
        Operation::ResignGuardian,
        Operation::SetWeightThreshold,
        Operation::StartRewardStream,
        Operation::TogglePause,
        Operation::RecordFailure,
        Operation::ResetCircuitBreaker,
        Operation::UpgradeContract,
    ];

    for op in ops {
        let cost = client.get_estimated_cost(&op);
        assert!(
            cost > BASE_OVERHEAD,
            "{:?} cost {} is not above the base invocation overhead of {}",
            op, cost, BASE_OVERHEAD
        );
    }
}

#[test]
fn test_logic_version() {
    let (_env, _admin, _token, client) = setup();
    assert_eq!(client.get_version(), 1);
}

#[test]
fn test_get_snapshot_pagination() {
    let (env, admin, token, client) = setup();

    let _g1 = add_guardian_with_rep(&env, &token, &client, &admin, 100);
    let _g2 = add_guardian_with_rep(&env, &token, &client, &admin, 200);

    client.register_task(&admin, &1u64);
    client.register_task(&admin, &2u64);

    // Get snapshot with offset=0, limit=1
    let snap1 = client.get_snapshot(&0u32, &1u32);
    assert_eq!(snap1.guardians.len(), 1);
    assert_eq!(snap1.tasks.len(), 1);

    // Get snapshot with offset=1, limit=1
    let snap2 = client.get_snapshot(&1u32, &1u32);
    assert_eq!(snap2.guardians.len(), 1);
    assert_eq!(snap2.tasks.len(), 1);

    // Ensure they contain disjoint keys
    let g1_key = snap1.guardians.keys().get(0).unwrap();
    let g2_key = snap2.guardians.keys().get(0).unwrap();
    assert_ne!(g1_key, g2_key);

    let t1_key = snap1.tasks.keys().get(0).unwrap();
    let t2_key = snap2.tasks.keys().get(0).unwrap();
    assert_ne!(t1_key, t2_key);
}

// ─── Re-entrant testing mock and test case ─────────────────────────

#[contract]
pub struct MockReentrantVault;

#[contractimpl]
impl MockReentrantVault {
    pub fn set_vero(env: Env, vero: Address) {
        env.storage().instance().set(&soroban_sdk::symbol_short!("vero"), &vero);
    }

    pub fn is_locked(env: Env) -> bool {
        env.storage().instance().get(&soroban_sdk::symbol_short!("locked")).unwrap_or(false)
    }

    pub fn release_funds(env: Env, task_id: u64) {
        let vero: Address = env.storage().instance().get(&soroban_sdk::symbol_short!("vero")).unwrap();
        let client = VeroCoreClient::new(&env, &vero);
        let guardian = Address::generate(&env);
        let reenter_result = client.try_vote(&guardian, &task_id);
        
        let is_locked = reenter_result.is_err();
        env.storage().instance().set(&soroban_sdk::symbol_short!("locked"), &is_locked);
    }
}

#[test]
fn test_strict_reentrancy_guards_prevent_reentry() {
    let (env, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &100u64);

    let vault_id = env.register_contract(None, MockReentrantVault);
    let vault_client = MockReentrantVaultClient::new(&env, &vault_id);
    
    vault_client.set_vero(&client.address);
    client.set_vault_address(&admin, &vault_id);

    let g = add_guardian_with_rep(&env, &token, &client, &admin, 100);
    client.register_task(&admin, &10u64);
    lock_for_guardian(&env, &token, &client, &g, 101);

    // This vote will resolve the task and call vault_client.release_funds,
    // which attempts to reenter VeroCore via try_vote
    client.vote(&g, &10u64);

    // Verify the vault mock recorded that the reentrant call was locked
    assert!(vault_client.is_locked());
}

// ─── Zero Address Input Checks ─────────────────────────────────────

#[test]
fn test_zero_address_input_rejections() {
    let env = Env::default();
    env.mock_all_auths();
    
    // We register the contract but don't initialize it yet
    let contract_id = env.register_contract(None, vero_core_contracts::VeroCore);
    let client = VeroCoreClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    let zero_contract = Address::from_string(&soroban_sdk::String::from_str(&env, "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4"));
    let zero_account = Address::from_string(&soroban_sdk::String::from_str(&env, "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"));

    // 1. Initialize checks
    assert!(client.try_initialize(&zero_contract, &100i128).is_err());
    assert!(client.try_initialize(&zero_account, &100i128).is_err());

    // Initialize properly for remaining tests
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    client.initialize(&token.address(), &100i128);

    // 2. Add guardian checks
    assert!(client.try_add_guardian(&admin, &zero_contract).is_err());
    assert!(client.try_add_guardian(&admin, &zero_account).is_err());

    // 3. Set vault address checks
    assert!(client.try_set_vault_address(&admin, &zero_contract).is_err());
    assert!(client.try_set_vault_address(&admin, &zero_account).is_err());

    // 4. Start reward stream checks
    let drips_addr = Address::generate(&env);
    let contributor = Address::generate(&env);

    assert!(client.try_start_reward_stream(&admin, &zero_contract, &contributor, &1u64).is_err());
    assert!(client.try_start_reward_stream(&admin, &zero_account, &contributor, &1u64).is_err());
    assert!(client.try_start_reward_stream(&admin, &drips_addr, &zero_contract, &1u64).is_err());
    assert!(client.try_start_reward_stream(&admin, &drips_addr, &zero_account, &1u64).is_err());
}

// ─── Token and Drips Reentrancy Protection tests ───────────────────

mod reentrant_token_mock {
    use super::*;

    #[contract]
    pub struct MockReentrantToken;

    #[contractimpl]
    impl MockReentrantToken {
        pub fn set_vero(env: Env, vero: Address) {
            env.storage().instance().set(&soroban_sdk::symbol_short!("vero"), &vero);
        }

        pub fn set_target(env: Env, target: soroban_sdk::Symbol) {
            env.storage().instance().set(&soroban_sdk::symbol_short!("target"), &target);
        }

        pub fn is_locked(env: Env) -> bool {
            env.storage().instance().get(&soroban_sdk::symbol_short!("locked")).unwrap_or(false)
        }

        pub fn transfer(env: Env, from: Address, _to: Address, amount: i128) {
            let vero: Address = env.storage().instance().get(&soroban_sdk::symbol_short!("vero")).unwrap();
            let client = VeroCoreClient::new(&env, &vero);
            let target: soroban_sdk::Symbol = env.storage().instance().get(&soroban_sdk::symbol_short!("target")).unwrap();
            
            if target == soroban_sdk::Symbol::new(&env, "none") {
                return;
            }

            let reenter_result = if target == soroban_sdk::Symbol::new(&env, "lock") {
                client.try_lock_tokens(&from, &amount)
            } else if target == soroban_sdk::Symbol::new(&env, "resign") {
                client.try_resign_guardian(&from)
            } else if target == soroban_sdk::Symbol::new(&env, "unlock") {
                client.try_unlock_tokens(&from)
            } else {
                panic!("invalid target");
            };
            
            let is_locked = reenter_result.is_err();
            env.storage().instance().set(&soroban_sdk::symbol_short!("locked"), &is_locked);
        }

        pub fn balance(_env: Env, _id: Address) -> i128 {
            1000000i128
        }
    }
}

mod reentrant_drips_mock {
    use super::*;

    #[contract]
    pub struct MockReentrantDrips;

    #[contractimpl]
    impl MockReentrantDrips {
        pub fn set_vero(env: Env, vero: Address) {
            env.storage().instance().set(&soroban_sdk::symbol_short!("vero"), &vero);
        }

        pub fn is_locked(env: Env) -> bool {
            env.storage().instance().get(&soroban_sdk::symbol_short!("locked")).unwrap_or(false)
        }

        pub fn start_stream(
            env: Env,
            contributor: Address,
            task_id: u64,
            _resolution_status: u32,
        ) {
            let vero: Address = env.storage().instance().get(&soroban_sdk::symbol_short!("vero")).unwrap();
            let client = VeroCoreClient::new(&env, &vero);
            let admin = Address::generate(&env);
            let drips_addr = env.current_contract_address();
            
            let reenter_result = client.try_start_reward_stream(&admin, &drips_addr, &contributor, &task_id);
            
            let is_locked = reenter_result.is_err();
            env.storage().instance().set(&soroban_sdk::symbol_short!("locked"), &is_locked);
        }
    }
}

#[test]
fn test_reentrancy_lock_tokens_prevented() {
    let env = Env::default();
    env.mock_all_auths();
    
    let reentrant_token_id = env.register_contract(None, reentrant_token_mock::MockReentrantToken);
    let token_client = reentrant_token_mock::MockReentrantTokenClient::new(&env, &reentrant_token_id);
    
    let contract_id = env.register_contract(None, vero_core_contracts::VeroCore);
    let client = VeroCoreClient::new(&env, &contract_id);
    
    token_client.set_vero(&client.address);
    token_client.set_target(&soroban_sdk::Symbol::new(&env, "lock"));
    
    let admin = Address::generate(&env);
    client.initialize(&reentrant_token_id, &100i128);
    
    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);
    
    client.lock_tokens(&guardian, &101i128);
    
    assert!(token_client.is_locked());
}

#[test]
fn test_reentrancy_resign_guardian_prevented() {
    let env = Env::default();
    env.mock_all_auths();
    
    let reentrant_token_id = env.register_contract(None, reentrant_token_mock::MockReentrantToken);
    let token_client = reentrant_token_mock::MockReentrantTokenClient::new(&env, &reentrant_token_id);
    
    let contract_id = env.register_contract(None, vero_core_contracts::VeroCore);
    let client = VeroCoreClient::new(&env, &contract_id);
    
    token_client.set_vero(&client.address);
    token_client.set_target(&soroban_sdk::Symbol::new(&env, "none"));
    
    let admin = Address::generate(&env);
    client.initialize(&reentrant_token_id, &100i128);
    
    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);
    
    // lock tokens cleanly
    client.lock_tokens(&guardian, &200i128);
    
    // set target to resign for reentrancy
    token_client.set_target(&soroban_sdk::Symbol::new(&env, "resign"));
    
    client.resign_guardian(&guardian);
    
    assert!(token_client.is_locked());
}

#[test]
fn test_reentrancy_unlock_tokens_prevented() {
    let env = Env::default();
    env.mock_all_auths();
    
    let reentrant_token_id = env.register_contract(None, reentrant_token_mock::MockReentrantToken);
    let token_client = reentrant_token_mock::MockReentrantTokenClient::new(&env, &reentrant_token_id);
    
    let contract_id = env.register_contract(None, vero_core_contracts::VeroCore);
    let client = VeroCoreClient::new(&env, &contract_id);
    
    token_client.set_vero(&client.address);
    token_client.set_target(&soroban_sdk::Symbol::new(&env, "none"));
    
    let _admin = Address::generate(&env);
    client.initialize(&reentrant_token_id, &100i128);
    
    let non_guardian = Address::generate(&env);
    
    // lock tokens cleanly
    client.lock_tokens(&non_guardian, &200i128);
    
    // set target to unlock for reentrancy
    token_client.set_target(&soroban_sdk::Symbol::new(&env, "unlock"));
    
    client.unlock_tokens(&non_guardian);
    
    assert!(token_client.is_locked());
}

#[test]
fn test_reentrancy_start_reward_stream_prevented() {
    let env = Env::default();
    env.mock_all_auths();
    
    // Set up standard mock token first
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token.address();
    
    let contract_id = env.register_contract(None, vero_core_contracts::VeroCore);
    let client = VeroCoreClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    client.initialize(&token_addr, &100i128);
    client.set_weight_threshold(&admin, &100u64);
    
    let g = Address::generate(&env);
    client.add_guardian(&admin, &g);
    client.set_reputation(&admin, &g, &100u64);
    
    let asset_client = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);
    asset_client.mint(&g, &101i128);
    client.lock_tokens(&g, &101i128);
    
    client.register_task(&admin, &42u64);
    client.vote(&g, &42u64); // resolves task
    
    let reentrant_drips_id = env.register_contract(None, reentrant_drips_mock::MockReentrantDrips);
    let drips_client = reentrant_drips_mock::MockReentrantDripsClient::new(&env, &reentrant_drips_id);
    drips_client.set_vero(&client.address);
    
    let contributor = Address::generate(&env);
    client.start_reward_stream(&admin, &reentrant_drips_id, &contributor, &42u64);
    
    assert!(drips_client.is_locked());
}
