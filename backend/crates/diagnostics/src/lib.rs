//! Independently compiled capability boundary.
pub mod shared {
    #[cfg(test)]
    pub use relaypulse_storage::shared::persistence;
    pub mod diagnostics;
    pub use relaypulse_runtime::shared::environment;
    pub use relaypulse_runtime::shared::file_output;
    pub use relaypulse_runtime::shared::state_changes;
}
pub use shared::diagnostics::*;
