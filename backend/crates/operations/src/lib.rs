extern crate self as relaypulse_operations;
pub mod shared {
    pub use relaypulse_runtime::shared::environment;
    pub use relaypulse_runtime::shared::file_output;
    pub use relaypulse_runtime::shared::state_changes;
    pub use relaypulse_storage::shared::persistence;
}
pub mod modules {
    pub mod operation_logs;
}
pub use modules::operation_logs::*;

#[cfg(test)]
#[path = "../../../tests/support/action_definitions.rs"]
mod test_actions;
