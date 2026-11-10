//! Independently compiled capability boundary.
pub mod shared {
    pub mod persistence;
    pub use relaypulse_runtime::shared::environment;
    pub use relaypulse_runtime::shared::file_output;
    pub use relaypulse_runtime::shared::state_changes;
}
pub use shared::persistence::*;
