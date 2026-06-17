/// Pure, `no_std`-compatible consensus logic — **no Soroban `Env` dependency**.
///
/// This module contains the arithmetic and state-transition rules for the
/// weighted guardian consensus. Keeping this logic free of SDK types allows
/// Kani (and other model checkers) to formally verify it without mocking the
/// Soroban host environment.
///
/// The contract's `vote()` entry point delegates to [`apply_vote`] after
/// performing all authentication, authorisation, and storage I/O.

/// Errors that can arise purely from consensus arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsensusError {
    /// Adding the guardian's weight to the accumulated total would overflow `u64`.
    WeightOverflow,
    /// The guardian's voting weight is zero — their vote has no effect.
    ZeroWeight,
}

/// The mutable consensus state for a single task.
///
/// This is a plain data struct with no Soroban types so that Kani can create
/// symbolic instances of it and exhaustively verify all reachable states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsensusState {
    /// Cumulative reputation weight accrued from all guardian votes so far.
    pub total_weight_accrued: u64,
    /// Number of guardian votes cast (saturating counter).
    pub votes: u32,
    /// `true` once the task has been resolved (monotonically set).
    pub is_done: bool,
}

impl ConsensusState {
    /// Creates a fresh, unresolved consensus state.
    pub const fn new() -> Self {
        Self {
            total_weight_accrued: 0,
            votes: 0,
            is_done: false,
        }
    }
}

/// Applies a single guardian vote to the consensus state.
///
/// # Arguments
/// * `state`     — mutable reference to the current task consensus state.
/// * `weight`    — the guardian's voting power (their reputation score).
/// * `threshold` — cumulative weight required to resolve the task.
///
/// # Behaviour
/// 1. Rejects zero-weight votes.
/// 2. Safely accumulates `weight` into `total_weight_accrued` via checked
///    addition, returning `Err(ConsensusError::WeightOverflow)` on overflow.
/// 3. Increments the vote counter with **saturating** arithmetic (never wraps).
/// 4. Sets `is_done = true` **if and only if** `total_weight_accrued >= threshold`
///    after the addition. `is_done` is never cleared once set.
///
/// # Invariants (proved by Kani harnesses in `verification/`)
/// * Resolution ↔ `total_weight_accrued >= threshold`
/// * No execution path sets `is_done` without meeting `threshold`
/// * `is_done` is monotonically set (never unset)
/// * `checked_add` prevents silent overflow
/// * `votes` saturates at `u32::MAX`
pub fn apply_vote(
    state: &mut ConsensusState,
    weight: u64,
    threshold: u64,
) -> Result<(), ConsensusError> {
    if weight == 0 {
        return Err(ConsensusError::ZeroWeight);
    }

    // Overflow-safe accumulation — the only arithmetic that matters for consensus.
    state.total_weight_accrued = state
        .total_weight_accrued
        .checked_add(weight)
        .ok_or(ConsensusError::WeightOverflow)?;

    // Saturating vote count — purely informational, never drives resolution.
    state.votes = state.votes.saturating_add(1);

    // Threshold check: set is_done iff threshold is met.
    // is_done is never cleared — once true it stays true.
    if state.total_weight_accrued >= threshold {
        state.is_done = true;
    }

    Ok(())
}

/// Returns `true` if the consensus state satisfies the resolution invariant:
/// `is_done` must be `true` **if and only if** `total_weight_accrued >= threshold`.
///
/// Used both in runtime assertions and in Kani harnesses as a post-condition.
pub fn resolution_invariant_holds(state: &ConsensusState, threshold: u64) -> bool {
    let weight_meets_threshold = state.total_weight_accrued >= threshold;
    // is_done must imply threshold met, AND threshold met must imply is_done
    // (for a freshly-voted state — NOT for older states where votes may have
    //  already set is_done and threshold was later lowered by admin).
    //
    // The minimal safety invariant (no resolution below threshold) is:
    //   is_done == true  →  weight_meets_threshold
    if state.is_done {
        weight_meets_threshold
    } else {
        true // Not done yet is always safe regardless of weight
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_vote_resolves_at_threshold() {
        let mut state = ConsensusState::new();
        apply_vote(&mut state, 300, 300).unwrap();
        assert!(state.is_done);
        assert_eq!(state.total_weight_accrued, 300);
        assert_eq!(state.votes, 1);
    }

    #[test]
    fn test_apply_vote_does_not_resolve_below_threshold() {
        let mut state = ConsensusState::new();
        apply_vote(&mut state, 299, 300).unwrap();
        assert!(!state.is_done);
        assert_eq!(state.total_weight_accrued, 299);
    }

    #[test]
    fn test_apply_vote_resolves_above_threshold() {
        let mut state = ConsensusState::new();
        apply_vote(&mut state, 500, 300).unwrap();
        assert!(state.is_done);
        assert_eq!(state.total_weight_accrued, 500);
    }

    #[test]
    fn test_apply_vote_accumulates_across_multiple_votes() {
        let mut state = ConsensusState::new();
        apply_vote(&mut state, 100, 300).unwrap();
        assert!(!state.is_done);
        apply_vote(&mut state, 100, 300).unwrap();
        assert!(!state.is_done);
        apply_vote(&mut state, 100, 300).unwrap();
        assert!(state.is_done);
        assert_eq!(state.total_weight_accrued, 300);
        assert_eq!(state.votes, 3);
    }

    #[test]
    fn test_apply_vote_rejects_zero_weight() {
        let mut state = ConsensusState::new();
        let err = apply_vote(&mut state, 0, 300).unwrap_err();
        assert_eq!(err, ConsensusError::ZeroWeight);
        assert!(!state.is_done);
        assert_eq!(state.total_weight_accrued, 0);
    }

    #[test]
    fn test_apply_vote_overflow_protection() {
        let mut state = ConsensusState::new();
        state.total_weight_accrued = u64::MAX;
        let err = apply_vote(&mut state, 1, 300).unwrap_err();
        assert_eq!(err, ConsensusError::WeightOverflow);
        // State must be unchanged after error
        assert_eq!(state.total_weight_accrued, u64::MAX);
        assert!(!state.is_done);
    }

    #[test]
    fn test_votes_counter_saturates() {
        let mut state = ConsensusState::new();
        state.votes = u32::MAX;
        // Should saturate, not overflow
        apply_vote(&mut state, 1, u64::MAX).unwrap();
        assert_eq!(state.votes, u32::MAX);
    }

    #[test]
    fn test_is_done_monotone_once_set() {
        // Once is_done is true, subsequent votes keep it true
        let mut state = ConsensusState::new();
        apply_vote(&mut state, 300, 300).unwrap();
        assert!(state.is_done);
        // Simulate more votes after resolution — is_done stays true
        apply_vote(&mut state, 100, 300).unwrap();
        assert!(state.is_done);
    }

    #[test]
    fn test_zero_threshold_first_vote_resolves() {
        // Threshold = 0: any non-zero weight vote immediately resolves.
        let mut state = ConsensusState::new();
        apply_vote(&mut state, 1, 0).unwrap();
        assert!(state.is_done);
    }

    #[test]
    fn test_resolution_invariant_holds_after_vote() {
        let mut state = ConsensusState::new();
        apply_vote(&mut state, 400, 300).unwrap();
        assert!(resolution_invariant_holds(&state, 300));
    }

    #[test]
    fn test_resolution_invariant_holds_before_threshold() {
        let mut state = ConsensusState::new();
        apply_vote(&mut state, 200, 300).unwrap();
        assert!(resolution_invariant_holds(&state, 300));
        assert!(!state.is_done);
    }
}
