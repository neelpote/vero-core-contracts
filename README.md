# Vero Core Contracts

On-chain GitHub PR verification for the Stellar ecosystem. Guardians — trusted off-chain validators — cast weighted votes on registered tasks (pull requests). Once cumulative reputation weight meets a configurable threshold the task is marked done, creating a tamper-proof audit trail on Soroban.

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                         VeroContract                             │
│                                                                  │
│  initialize(token, threshold)                                    │
│  add_guardian(admin, guardian)                                   │
│  register_task(admin, task_id)                                   │
│  vote(guardian, task_id) ──► weight check ──► threshold check   │
│  get_task(task_id) ──► Task { id, votes, is_done, weight }       │
│                                                                  │
│  pause(admin) / unpause(admin) / toggle_pause(admin)            │
│  record_failure() ──► circuit breaker (auto-pause at >50)       │
│  reset_circuit_breaker(admin)                                    │
└──────────────────────────────────────┬───────────────────────────┘
                                       │ instance storage
                         ┌─────────────┴──────────────┐
                         │          DataKey            │
                         │  Guardian(Address)          │
                         │  Reputation(Address)        │
                         │  Task(u64)                  │
                         │  Voted(u64, Address)        │
                         │  Paused                     │
                         │  FailureCount               │
                         └─────────────────────────────┘
```

**Flow**

1. Admin calls `initialize` with a token address and lock threshold.
2. Admin registers a GitHub PR as a `Task` with a unique numeric ID.
3. Admin whitelists trusted validator addresses as guardians and assigns reputation scores.
4. Guardians lock tokens above the threshold, then call `vote`.
5. Each vote adds the guardian's reputation weight to `total_weight_accrued`.
6. When `total_weight_accrued >= weight_threshold` (default 300) the task's `is_done` flips to `true`.

---

## Modules

| Module | Responsibility |
|---|---|
| `types` | `Task`, `DataKey`, `ContractError`, `RewardStream` |
| `guardian` | Guardian registry with TTL-extended instance storage |
| `task` | Task registration and retrieval |
| `reputation` | Guardian reputation scores and voting power calculation |
| `circuit_breaker` | Emergency halt: `require_not_paused`, `record_failure`, `reset` |
| `reentrancy` | Mutex lock/unlock guarding `vote` and `register_task` |
| `drips` | Cross-contract reward stream initiation via Drips protocol |
| `vault` | Cross-contract escrow release on task resolution |
| `events` | On-chain event emission |
| `lib` | Public contract surface and `vote` orchestration |

---

## Quick Start

### Prerequisites

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked soroban-cli
```

### Build

```bash
cargo build --target wasm32-unknown-unknown --release
```

### Test

```bash
cargo test
```

---

## Code Snippets

### Initialize the contract

```rust
client.initialize(&token_address, &100i128); // lock threshold = 100
```

### Add a guardian and set reputation

```rust
client.add_guardian(&admin, &validator_address);
client.set_reputation(&admin, &validator_address, &300u64); // score = 300
```

### Lock tokens (guardian must do this before voting)

```rust
client.lock_tokens(&guardian, &150i128); // amount > threshold
```

### Register a task

```rust
client.register_task(&admin, &pr_number);
```

### Cast a vote

```rust
client.vote(&guardian, &pr_number)?;
```

### Query task state

```rust
let task = client.get_task(&pr_number).unwrap();
assert!(task.is_done); // true once weight threshold is reached
```

---

## Storage Design

All state lives in **instance storage** — scoped to the contract instance and extended with a 100 000-ledger TTL window on every guardian write. Keys are typed via the `DataKey` enum so there are no raw string collisions.

```rust
pub enum DataKey {
    Guardian(Address),      // bool — is this address a guardian?
    Reputation(Address),    // u64 — reputation score
    Task(u64),              // Task struct
    Voted(u64, Address),    // bool — has this guardian voted on this task?
    WeightThreshold,        // u64 — cumulative weight required to resolve
    TokenAddress,           // Address — locked token contract
    LockThreshold,          // i128 — minimum locked balance to vote
    LockedBalance(Address), // i128 — tokens locked by a guardian
    Lock,                   // re-entrancy mutex
    FailureCount,           // u32 — circuit breaker failure counter
    Paused,                 // bool — emergency halt flag
    VaultAddress,           // Address — escrow vault contract
    RewardStream(u64),      // RewardStream — active drip stream for a task
}
```

---

## Error Codes

| Code | Variant | Meaning |
|---|---|---|
| 1 | `NotAuthorized` | Caller is not a registered guardian or admin |
| 2 | `DuplicateVote` | Guardian already voted on this task |
| 3 | `TaskNotVerified` | Task is not yet resolved; cannot start reward stream |
| 4 | `StreamAlreadyActive` | A reward stream for this task already exists |
| 5 | `DripsCallFailed` | Cross-contract call to Drips protocol reverted |
| 6 | `AlreadyInitialized` | Contract has already been initialized |
| 7 | `NotInitialized` | Contract has not been initialized |
| 8 | `NoReputationScore` | Guardian has no reputation score assigned |
| 9 | `ZeroWeightVote` | Guardian's reputation score is zero |
| 10 | `WeightOverflow` | Adding vote weight would overflow u64 |
| 11 | `InsufficientLockedBalance` | Guardian's locked balance does not exceed the threshold |
| 12 | `StillGuardian` | Cannot unlock tokens while still registered as a guardian |
| 13 | `NotGuardian` | Address is not a registered guardian |
| 14 | `Locked` | Re-entrancy guard is active |
| 15 | `ContractPaused` | Contract is paused; all state-changing calls are blocked |
| 16 | `EscrowUnavailable` | Cross-contract call to vault/escrow reverted |

---

## Emergency Halt (Circuit Breaker)

The contract has a two-track emergency halt system that allows an admin to immediately freeze all state-changing operations if a vulnerability is discovered, without requiring a contract migration.

### Manual pause / unpause

```rust
// Immediately block all state-changing entry points
client.pause(&admin);

// Restore normal operation
client.unpause(&admin);

// Or toggle the current state
client.toggle_pause(&admin);

// Check current state
let frozen: bool = client.is_paused();
```

Both `pause` and `unpause` require `admin.require_auth()`. No other address can call them.

When paused, any call to `register_task`, `vote`, `add_guardian`, `set_reputation`, `set_weight_threshold`, or `start_reward_stream` returns `Err(ContractError::ContractPaused)` immediately.

### Automatic circuit breaker

Off-chain monitors can report observed failures via `record_failure`. After **50 cumulative failures** the contract pauses itself automatically and emits a `cb_trip` event.

```rust
// Called by off-chain monitor after observing a failed invocation
client.record_failure();
```

To resume after investigation:

```rust
// Resets the failure counter and unpauses
client.reset_circuit_breaker(&admin);
```

### Emergency halt procedure

1. **Detect** — Either trigger `pause` manually, or wait for `record_failure` to trip the breaker at >50 failures.
2. **Verify** — Call `is_paused()` on-chain to confirm the contract is frozen.
3. **Investigate** — Audit storage state and transaction history off-chain.
4. **Remediate** — Deploy a patched WASM via `upgrade_contract` if needed.
5. **Resume** — Call `reset_circuit_breaker` (resets counter + unpauses) or `unpause` if the failure counter was not the trigger.

> **Security note:** Only the Multi-Sig admin key can call `pause`, `unpause`, and `reset_circuit_breaker`. The `record_failure` entry point is permissionless so that any observer can report failures, but it only increments a counter — it cannot directly manipulate task or guardian state.

---

## License

Apache-2.0
