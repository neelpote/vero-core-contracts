# Pull Request: Strict Reentrancy Protection & Zero-Address Input Validation

## Description
This pull request implements two critical security enhancements for the Vero Core Contracts under the **GrantFox OSS Campaign (Official Campaign)**:

1. **Strict Reentrancy Protection (Closes #67)**:
   - Introduces a declarative `non_reentrant!` macro backed by a RAII `ReentrancyGuard` pattern.
   - Automatically drops/releases the reentrancy lock on function exit (even during early returns or `?` error propagation), eliminating manual `reentrancy::unlock` calls.
   - Applies `non_reentrant!` guard to all state-changing external calls: `lock_tokens`, `resign_guardian`, `unlock_tokens`, and `start_reward_stream` (in addition to `vote`, `register_tasks`, and `cancel_task`).
   - Enforces the Check-Effects-Interactions (CEI) pattern on these functions to update storage state (e.g. updating locked balance or setting task states) *before* performing any external token transfer or contract invocation.
2. **Zero-Address Input Validation (Closes #78)**:
   - Rejects zero-address contract IDs (StrKey: `CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4`) and Stellar null accounts (StrKey: `GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF`) on input parameters for token, guardian, vault, drips, and contributor addresses.
   - Prevents funds from accidentally being sent to a burn/null address.

Both issues are tracked and rewarded via the **GrantFox OSS** campaign.

---

## Key Changes

### 1. Reentrancy Guard & Macro
- **[src/reentrancy.rs](file:///Users/neelsubhashpote/corecontracts/src/reentrancy.rs)**:
  - Added `ReentrancyGuard<'a>` and implemented the `Drop` trait to call `reentrancy::unlock`.
  - Added the `non_reentrant!` macro.
- **[src/lib.rs](file:///Users/neelsubhashpote/corecontracts/src/lib.rs)**:
  - Made `reentrancy` module public (`pub mod reentrancy`).
  - Refactored `vote`, `lock_tokens`, `resign_guardian`, `unlock_tokens`, and `start_reward_stream` to use `non_reentrant!(&env)`.
  - Enforced Check-Effects-Interactions (CEI) state writes before external calls in `lock_tokens`, `resign_guardian`, `unlock_tokens`, and `vote`.
- **[src/task.rs](file:///Users/neelsubhashpote/corecontracts/src/task.rs)**:
  - Refactored `register_tasks` and `cancel_task` to use `crate::non_reentrant!(env)`.
- **[src/drips.rs](file:///Users/neelsubhashpote/corecontracts/src/drips.rs)**:
  - Enforced CEI by updating all storage mappings before executing the external `env.invoke_contract` call.

### 2. Zero-Address input rejection
- **[src/types.rs](file:///Users/neelsubhashpote/corecontracts/src/types.rs)**:
  - Added `InvalidAddress = 22` to `ContractError` enum.
- **[src/lib.rs](file:///Users/neelsubhashpote/corecontracts/src/lib.rs)**:
  - Implemented `require_not_zero(env: &Env, addr: &Address) -> Result<(), ContractError>` validating against the base32 zero contract ID and the Stellar zero account.
  - Integrated `require_not_zero` in `initialize`, `add_guardian`, `set_vault_address`, and `start_reward_stream`.

### 3. Tests & Verification
- **[tests/test.rs](file:///Users/neelsubhashpote/corecontracts/tests/test.rs)**:
  - Added `test_strict_reentrancy_guards_prevent_reentry` using a mock contract to attempt to reenter the core via `vote` and assert it fails.
  - Added `test_reentrancy_lock_tokens_prevented`, `test_reentrancy_resign_guardian_prevented`, `test_reentrancy_unlock_tokens_prevented`, and `test_reentrancy_start_reward_stream_prevented` using module-wrapped `MockReentrantToken` and `MockReentrantDrips` to verify reentrancy protection on all state-changing external calls.
  - Added `test_zero_address_input_rejections` to verify that all-zero contracts and Stellar accounts are rejected with `ContractError::InvalidAddress` across all validation points.

---

## Testing & Verification
- Ran the entire test suite locally:
  ```bash
  cargo test
  ```
- **Results**: All 55 tests compiled and passed successfully.
