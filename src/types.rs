use soroban_sdk::{contracterror, contracttype, Address};

#[contracttype]
#[derive(Clone)]
pub struct Task {
    pub id: u64,
    pub votes: u32,
    pub is_done: bool,
    /// Cumulative reputation weight accrued from all guardian votes.
    /// Consensus is reached when this meets or exceeds the weight threshold.
    pub total_weight_accrued: u64,
}

/// Represents an active reward stream initiated via the Drips protocol
/// after a task has been verified by guardian consensus.
#[contracttype]
#[derive(Clone)]
pub struct RewardStream {
    pub task_id: u64,
    pub contributor: Address,
    pub drips_contract: Address,
    pub active: bool,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Guardian(Address),
    Reputation(Address),
    WeightThreshold,
    Task(u64),
    Voted(u64, Address), // (task_id, guardian)
    Admin,
    DripsAddress,
    VaultAddress,
    RewardStream(u64), // keyed by task_id
    TokenAddress,
    LockThreshold,
    LockedBalance(Address),
    Lock,              // re-entrancy mutex
    WeightThreshold,
    Reputation(Address),   // u64 reputation score for a guardian
    FailureCount,          // circuit breaker failure counter
    Paused,                // circuit breaker pause flag
}

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractError {
    NotAuthorized = 1,
    DuplicateVote = 2,
    NoReputationScore = 8,
    ZeroWeightVote = 9,
    WeightOverflow = 10,
    TaskNotVerified = 3,
    StreamAlreadyActive = 4,
    DripsCallFailed = 5,
    AlreadyInitialized = 6,
    NotInitialized = 7,
    InsufficientLockedBalance = 8,
    StillGuardian = 9,
    NotGuardian = 10,
    Locked = 6,
    NoReputationScore = 7,
    ZeroWeightVote = 8,
    WeightOverflow = 9,
    ContractPaused = 10,
}
