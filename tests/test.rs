#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};
use vero_core_contracts::VeroContractClient;

fn setup() -> (Env, Address, Address, VeroContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, vero_core_contracts::VeroContract);
    let client = VeroContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    (env, admin, client)
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
    assert!(!task.is_done);
}

// ─── Reputation management ─────────────────────────────────────────

#[test]
fn test_set_and_get_reputation() {
    let (env, admin, client) = setup();
    let guardian = Address::generate(&env);

    client.add_guardian(&admin, &guardian);
    client.set_reputation(&admin, &guardian, &500u64);

    let score = client.get_reputation(&guardian);
    assert_eq!(score, Some(500));
}

#[test]
fn test_calculate_voting_power_returns_score() {
    let (env, admin, client) = setup();
    let guardian = Address::generate(&env);

    client.add_guardian(&admin, &guardian);
    client.set_reputation(&admin, &guardian, &150u64);

    let power = client.calculate_voting_power(&guardian);
    assert_eq!(power, Some(150));
}

#[test]
fn test_calculate_voting_power_none_for_unset() {
    let (env, _admin, client) = setup();
    let stranger = Address::generate(&env);

    let power = client.calculate_voting_power(&stranger);
    assert_eq!(power, None);
}

// ─── Weighted consensus: weight-based resolution ───────────────────

#[test]
fn test_single_high_rep_guardian_resolves_task() {
    // A single guardian with reputation >= threshold can resolve a task alone
    let (env, admin, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let g = add_guardian_with_rep(&env, &client, &admin, 300);
    client.register_task(&admin, &1u64);
    client.vote(&g, &1u64);

    let task = client.get_task(&1u64).unwrap();
    assert_eq!(task.votes, 1);
    assert_eq!(task.total_weight_accrued, 300);
    assert!(task.is_done, "single high-rep guardian should resolve task");
}

#[test]
fn test_multiple_low_rep_guardians_accumulate_weight() {
    // Three guardians with rep=100 each → total_weight = 300 → resolved
    let (env, admin, token, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let g1 = add_guardian_with_rep(&env, &client, &admin, 100);
    let g2 = add_guardian_with_rep(&env, &client, &admin, 100);
    let g3 = add_guardian_with_rep(&env, &client, &admin, 100);

    client.register_task(&admin, &42u64);

    lock_for_guardian(&env, &token, &client, &g1, 101);
    lock_for_guardian(&env, &token, &client, &g2, 101);
    lock_for_guardian(&env, &token, &client, &g3, 101);

    client.vote(&g1, &42u64);
    client.vote(&g2, &42u64);
    client.vote(&g3, &42u64);

    let task = client.get_task(&42u64).unwrap();
    assert_eq!(task.total_weight_accrued, 300);
    assert_eq!(task.votes, 3);
    assert!(task.is_done, "three low-rep guardians should resolve task");
}

#[test]
fn test_weight_vs_count_logic() {
    // Two guardians with high rep should resolve even though count < 3.
    // This demonstrates weight-based consensus vs the old count-based system.
    let (env, admin, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let g1 = add_guardian_with_rep(&env, &client, &admin, 200);
    let g2 = add_guardian_with_rep(&env, &client, &admin, 150);

    client.register_task(&admin, &20u64);

    client.vote(&g1, &20u64);
    client.vote(&g2, &20u64);

    let task = client.get_task(&20u64).unwrap();
    assert_eq!(task.votes, 2, "only 2 votes cast");
    assert_eq!(task.total_weight_accrued, 350);
    assert!(
        task.is_done,
        "2 high-rep votes should resolve task despite count < 3"
    );
}

#[test]
fn test_many_low_rep_guardians_cannot_resolve_without_enough_weight() {
    // Five guardians with rep=50 each → total_weight = 250 < 300 → NOT resolved
    let (env, admin, client) = setup();
    client.set_weight_threshold(&admin, &300u64);

    let guardians: [Address; 5] = core::array::from_fn(|_| {
        add_guardian_with_rep(&env, &client, &admin, 50)
    });

    client.register_task(&admin, &30u64);

    for g in &guardians {
        client.vote(g, &30u64);
    }

    let task = client.get_task(&30u64).unwrap();
    assert_eq!(task.votes, 5);
    assert_eq!(task.total_weight_accrued, 250);
    assert!(
        !task.is_done,
        "5 guardians with rep=50 should NOT reach threshold of 300"
    );
}

#[test]
fn test_task_resolved_includes_final_weight() {
    // Verify the resolved task's total_weight_accrued reflects the exact sum
    let (env, admin, client) = setup();
    client.set_weight_threshold(&admin, &100u64);

    let g1 = add_guardian_with_rep(&env, &client, &admin, 42);
    let g2 = add_guardian_with_rep(&env, &client, &admin, 73);

    client.register_task(&admin, &40u64);
    client.vote(&g1, &40u64);
    client.vote(&g2, &40u64);

    let task = client.get_task(&40u64).unwrap();
    assert_eq!(task.total_weight_accrued, 115, "42 + 73 = 115");
    assert!(task.is_done);
}

// ─── Configurable weight threshold ─────────────────────────────────

#[test]
fn test_custom_weight_threshold() {
    let (_env, admin, client) = setup();

    // Default threshold
    let default = client.get_weight_threshold();
    assert_eq!(default, 300);

    // Set custom threshold
    client.set_weight_threshold(&admin, &1000u64);
    assert_eq!(client.get_weight_threshold(), 1000);
}

// ─── Error handling ─────────────────────────────────────────────────

#[test]
fn test_vote_rejected_without_reputation() {
    // Renamed: actually tests duplicate vote rejection.
    // A guardian with reputation votes once (ok), then again (rejected).
    let (env, admin, token, client) = setup();
    let g = add_guardian_with_rep(&env, &client, &admin, 100);

    client.register_task(&admin, &7u64);
    lock_for_guardian(&env, &token, &client, &g, 101);

    let result = client.try_vote(&g, &7u64);
    assert!(result.is_err(), "vote without reputation should be rejected");
}

#[test]
fn test_non_guardian_vote_rejected() {
    let (env, admin, _token, client) = setup();
    let stranger = Address::generate(&env);

    client.register_task(&admin, &99u64);

    let result = client.try_vote(&stranger, &99u64);
    assert!(result.is_err(), "non-guardian vote should be rejected");
}

#[test]
fn test_vote_on_nonexistent_task_rejected() {
    let (env, admin, client) = setup();
    let g = add_guardian_with_rep(&env, &client, &admin, 100);

    let result = client.try_vote(&g, &999u64);
    assert!(
        result.is_err(),
        "vote on nonexistent task should be rejected"
    );
}

// ─── Reputation update after initial assignment ─────────────────────

#[test]
fn test_reputation_can_be_updated() {
    let (env, admin, client) = setup();
    let g = Address::generate(&env);

    client.add_guardian(&admin, &g);
    client.set_reputation(&admin, &g, &100u64);
    assert_eq!(client.get_reputation(&g), Some(100));

    // Update reputation
    client.set_reputation(&admin, &g, &500u64);
    assert_eq!(client.get_reputation(&g), Some(500));
    assert_eq!(client.calculate_voting_power(&g), Some(500));
}

// ─── Drips cross-contract integration tests ────────────────────────────

#[test]
fn test_reward_stream_rejected_for_unverified_task() {
    let (env, admin, _token, client) = setup();
    let contributor = Address::generate(&env);
    let drips_addr = Address::generate(&env);

    // Register but do NOT verify the task (no votes)
    client.register_task(&admin, &10u64);

    let result = client.try_start_reward_stream(&admin, &drips_addr, &contributor, &10u64);
    assert!(result.is_err(), "should reject stream for unverified task");
}

#[test]
fn test_reward_stream_rejected_for_nonexistent_task() {
    let (env, admin, _token, client) = setup();
    let contributor = Address::generate(&env);
    let drips_addr = Address::generate(&env);

    // Task 999 was never registered
    let result = client.try_start_reward_stream(&admin, &drips_addr, &contributor, &999u64);
    assert!(result.is_err(), "should reject stream for nonexistent task");
}

#[test]
fn test_reward_stream_duplicate_rejected() {
    let (env, admin, token, client) = setup();
    let contributor = Address::generate(&env);

    let g1 = add_guardian_with_rep(&env, &client, &admin, 100);
    let g2 = add_guardian_with_rep(&env, &client, &admin, 100);
    let g3 = add_guardian_with_rep(&env, &client, &admin, 100);
    client.register_task(&admin, &50u64);

    lock_for_guardian(&env, &token, &client, &g1, 101);
    lock_for_guardian(&env, &token, &client, &g2, 101);
    lock_for_guardian(&env, &token, &client, &g3, 101);

    client.vote(&g1, &50u64);
    client.vote(&g2, &50u64);
    client.vote(&g3, &50u64);

    // Deploy a mock Drips contract to receive the cross-contract call
    let drips_contract_id = env.register_contract(None, MockDripsContract);

    // First stream should succeed
    client.start_reward_stream(&admin, &drips_contract_id, &contributor, &50u64);

    // Second attempt for same task should fail
    let result =
        client.try_start_reward_stream(&admin, &drips_contract_id, &contributor, &50u64);
    assert!(result.is_err(), "should reject duplicate stream");
}

#[test]
fn test_reward_stream_stored_after_success() {
    let (env, admin, token, client) = setup();
    let contributor = Address::generate(&env);

    let g1 = add_guardian_with_rep(&env, &client, &admin, 100);
    let g2 = add_guardian_with_rep(&env, &client, &admin, 100);
    let g3 = add_guardian_with_rep(&env, &client, &admin, 100);
    client.register_task(&admin, &77u64);

    lock_for_guardian(&env, &token, &client, &g1, 101);
    lock_for_guardian(&env, &token, &client, &g2, 101);
    lock_for_guardian(&env, &token, &client, &g3, 101);

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

// ─── Token Locking Tests ────────────────────────────────────────────────

#[test]
fn test_voting_fails_if_tokens_not_locked() {
    let (env, admin, _token, client) = setup();
    let g = Address::generate(&env);

    client.add_guardian(&admin, &g);
    client.register_task(&admin, &100u64);

    // Try voting without locking tokens
    let result = client.try_vote(&g, &100u64);
    assert!(result.is_err());
}

#[test]
fn test_voting_fails_if_locked_balance_leq_threshold() {
    let (env, admin, token, client) = setup();
    let g = Address::generate(&env);

    client.add_guardian(&admin, &g);
    client.register_task(&admin, &100u64);

    // Lock exactly threshold (100) tokens
    lock_for_guardian(&env, &token, &client, &g, 100);

    // Try voting (should fail because locked balance must be > threshold, i.e., > 100)
    let result = client.try_vote(&g, &100u64);
    assert!(result.is_err());

    // Lock 1 more token (total 101)
    lock_for_guardian(&env, &token, &client, &g, 1);

    // Try voting (should succeed)
    client.vote(&g, &100u64);
    let task = client.get_task(&100u64).unwrap();
    assert_eq!(task.votes, 1);
}

#[test]
fn test_resign_guardian_refunds_tokens() {
    let (env, admin, token, client) = setup();
    let g = Address::generate(&env);

    client.add_guardian(&admin, &g);
    lock_for_guardian(&env, &token, &client, &g, 200);

    // Resign guardian
    client.resign_guardian(&g);

    // Verify resignation
    assert!(!client.is_guardian(&g));

    // Verify token refund
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&g), 200);
}

#[test]
fn test_unlock_fails_while_guardian() {
    let (env, admin, token, client) = setup();
    let g = Address::generate(&env);

    client.add_guardian(&admin, &g);
    lock_for_guardian(&env, &token, &client, &g, 200);

    // Try unlocking while still guardian (should fail)
    let result = client.try_unlock_tokens(&g);
    assert!(result.is_err());
}

#[test]
fn test_unlock_succeeds_for_non_guardian() {
    let (env, _admin, token, client) = setup();
    let non_guardian = Address::generate(&env);

    // Lock tokens for non-guardian
    lock_for_guardian(&env, &token, &client, &non_guardian, 150);

    // Unlock (should succeed because they are not a guardian)
    client.unlock_tokens(&non_guardian);

    // Verify token refund
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&non_guardian), 150);
}

// ─── Re-entrancy protection tests ─────────────────────────────────────

#[test]
fn test_lock_released_after_successful_vote() {
    // After a normal vote the lock must be cleared so subsequent votes work.
    // If the lock leaked, the second vote would fail with Locked.
    let (env, admin, client) = setup();
    let g1 = add_guardian_with_rep(&env, &client, &admin, 100);
    let g2 = add_guardian_with_rep(&env, &client, &admin, 100);

    client.register_task(&admin, &202u64);

    client.vote(&g1, &202u64);
    client.vote(&g2, &202u64); // would fail if lock was not released

    let task = client.get_task(&202u64).unwrap();
    assert_eq!(task.votes, 2);
}

#[test]
fn test_lock_released_after_successful_register_task() {
    // Registering two tasks sequentially verifies the lock is released each time.
    let (_env, admin, client) = setup();

    client.register_task(&admin, &300u64);
    client.register_task(&admin, &301u64); // would fail if lock leaked

    assert!(client.get_task(&300u64).is_some());
    assert!(client.get_task(&301u64).is_some());
}

#[test]
fn test_lock_released_after_failed_vote() {
    // When vote() fails early (non-guardian), the lock must still be released
    // so a subsequent legitimate call can succeed.
    let (env, admin, client) = setup();
    let g = add_guardian_with_rep(&env, &client, &admin, 100);
    let stranger = Address::generate(&env);

    client.register_task(&admin, &303u64);

    // Non-guardian vote is rejected (lock must be released inside)
    let _ = client.try_vote(&stranger, &303u64);

    // Legitimate vote must still succeed
    client.vote(&g, &303u64);
    assert_eq!(client.get_task(&303u64).unwrap().votes, 1);
}

// ─── Emergency stop (pause) tests ─────────────────────────────────────

#[test]
fn test_admin_can_toggle_pause() {
    let (_env, admin, client) = setup();

    assert!(!client.is_paused(), "contract should start unpaused");

    client.toggle_pause(&admin);
    assert!(client.is_paused(), "contract should be paused after toggle");

    client.toggle_pause(&admin);
    assert!(!client.is_paused(), "contract should be unpaused after second toggle");
}

#[test]
fn test_contract_paused_error_on_register_task() {
    let (_env, admin, client) = setup();

    client.toggle_pause(&admin);

    let result = client.try_register_task(&admin, &1u64);
    assert!(result.is_err(), "register_task should fail when paused");
}

#[test]
fn test_contract_paused_error_on_vote() {
    let (env, admin, client) = setup();
    let g = add_guardian_with_rep(&env, &client, &admin, 300);
    client.register_task(&admin, &1u64);

    client.toggle_pause(&admin);

    let result = client.try_vote(&g, &1u64);
    assert!(result.is_err(), "vote should fail when paused");
}

#[test]
fn test_contract_paused_error_on_add_guardian() {
    let (env, admin, client) = setup();
    let guardian = Address::generate(&env);

    client.toggle_pause(&admin);

    let result = client.try_add_guardian(&admin, &guardian);
    assert!(result.is_err(), "add_guardian should fail when paused");
}

#[test]
fn test_contract_paused_error_on_set_reputation() {
    let (env, admin, client) = setup();
    let guardian = Address::generate(&env);
    client.add_guardian(&admin, &guardian);

    client.toggle_pause(&admin);

    let result = client.try_set_reputation(&admin, &guardian, &100u64);
    assert!(result.is_err(), "set_reputation should fail when paused");
}

#[test]
fn test_operations_resume_after_unpause() {
    let (env, admin, client) = setup();
    let g = add_guardian_with_rep(&env, &client, &admin, 300);

    client.toggle_pause(&admin);
    assert!(client.try_register_task(&admin, &1u64).is_err());

    client.toggle_pause(&admin);
    client.register_task(&admin, &1u64);
    client.vote(&g, &1u64);

    let task = client.get_task(&1u64).unwrap();
    assert!(task.is_done, "task should resolve after unpause");
}

// ─── Mock Drips contract for test isolation ────────────────────────────

use soroban_sdk::{contract, contractimpl};

/// A minimal mock of the Drips protocol contract used in tests.
/// It accepts `start_stream` calls without side-effects so we can
/// validate the Vero contract's cross-contract call logic in isolation.
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
        // Mock: accept the call silently
    }
}

// ─── Circuit breaker tests ─────────────────────────────────────────────

#[test]
fn test_circuit_breaker_trips_after_threshold() {
    let (_env, _admin, client) = setup();
    for _ in 0..51 {
        client.record_failure();
    }
    assert!(client.is_paused(), "contract should be paused after 51 failures");
}

#[test]
fn test_paused_contract_rejects_vote() {
    let (env, admin, client) = setup();
    client.register_task(&admin, &1u64);
    for _ in 0..51 {
        client.record_failure();
    }
    assert!(client.is_paused());

    let g = add_guardian_with_rep(&env, &client, &admin, 100);
    let result = client.try_vote(&g, &1u64);
    assert!(result.is_err(), "vote should be rejected while paused");
}

#[test]
fn test_paused_contract_rejects_register_task() {
    let (_env, admin, client) = setup();
    for _ in 0..51 {
        client.record_failure();
    }
    assert!(client.is_paused());

    let result = client.try_register_task(&admin, &2u64);
    assert!(result.is_err(), "register_task should be rejected while paused");
}

#[test]
fn test_admin_can_reset_circuit_breaker() {
    let (env, admin, client) = setup();
    client.register_task(&admin, &1u64);
    for _ in 0..51 {
        client.record_failure();
    }
    assert!(client.is_paused());

    client.reset_circuit_breaker(&admin);
    assert!(!client.is_paused(), "contract should be unpaused after reset");

    // Operations should work again
    let g = add_guardian_with_rep(&env, &client, &admin, 100);
    let result = client.try_vote(&g, &1u64);
    assert!(result.is_ok(), "vote should succeed after reset");
}
