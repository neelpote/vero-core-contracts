use soroban_sdk::{contracterror, contracttype, Address, Map, Vec};

#[contracttype]
#[derive(Clone)]
pub struct Task {
    pub id: u64,
    pub votes: u32,
    pub is_done: bool,
    pub total_weight_accrued: u64,
    pub is_cancelled: bool,
}

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
pub struct Snapshot {
    pub paused: bool,
    pub failure_count: u32,
    pub weight_threshold: u64,
    pub admin: Option<Address>,
    pub vault_address: Option<Address>,
    pub drips_address: Option<Address>,
    pub guardians: Map<Address, bool>,
    pub reputations: Map<Address, u64>,
    pub tasks: Map<u64, Task>,
    pub votes: Map<(u64, Address), bool>,
    pub reward_streams: Map<u64, RewardStream>,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Guardian(Address),
    Reputation(Address),
    WeightThreshold,
    Task(u64),
    Voted(u64, Address),
    Admin,
    DripsAddress,
    VaultAddress,
    RewardStream(u64),
    TokenAddress,
    LockThreshold,
    LockedBalance(Address),
    Lock,
    FailureCount,
    Paused,
    AllGuardians,
    AllTasks,
    AllVotes,
    AllRewardStreams,
}

/// Every public write operation exposed by VeroContract.
/// Used as the argument to `get_estimated_cost` so callers can query
/// the estimated instruction-unit cost before submitting a transaction.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    /// `register_task` — 2 storage writes (reentrancy lock + task entry).
    RegisterTask = 0,
    /// `vote` — most complex path: 5+ reads, 2 writes, conditional cross-contract call.
    Vote = 1,
    /// `add_guardian` — 1 write + admin auth check.
    AddGuardian = 2,
    /// `set_reputation` — 1 write + admin auth check.
    SetReputation = 3,
    /// `lock_tokens` — token cross-contract transfer + 2 storage writes.
    LockTokens = 4,
    /// `unlock_tokens` — same structure as `lock_tokens`.
    UnlockTokens = 5,
    /// `resign_guardian` — 2 writes + conditional token transfer.
    ResignGuardian = 6,
    /// `set_weight_threshold` — 1 write + admin auth.
    SetWeightThreshold = 7,
    /// `start_reward_stream` — 2 reads + cross-contract call + 1 write.
    StartRewardStream = 8,
    /// `toggle_pause` / `pause` / `unpause` — 1 read + 1 write + event emission.
    TogglePause = 9,
    /// `record_failure` — 1 read + 1 write + conditional second write.
    RecordFailure = 10,
    /// `reset_circuit_breaker` — 2 writes + admin auth.
    ResetCircuitBreaker = 11,
    /// `upgrade_contract` — WASM upgrade; highest fixed platform cost.
    UpgradeContract = 12,
}

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractError {
    NotAuthorized = 1,
    DuplicateVote = 2,
    TaskNotVerified = 3,
    StreamAlreadyActive = 4,
    DripsCallFailed = 5,
    AlreadyInitialized = 6,
    NotInitialized = 7,
    NoReputationScore = 8,
    ZeroWeightVote = 9,
    WeightOverflow = 10,
    InsufficientLockedBalance = 11,
    StillGuardian = 12,
    NotGuardian = 13,
    Locked = 14,
    ContractPaused = 15,
    EscrowUnavailable = 16,
    TaskCancelled = 17,
}
