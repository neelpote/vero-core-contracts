use soroban_sdk::{contractclient, Env};

#[contractclient(name = "VaultClient")]
pub trait Vault {
    fn release_funds(env: Env, task_id: u64);
}
