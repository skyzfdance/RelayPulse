//! Independently compiled capability boundary.
pub mod shared {
    pub use relaypulse_diagnostics::shared::diagnostics;
    pub use relaypulse_storage::shared::persistence;
}
pub mod modules {
    pub mod notifications;
}
pub use modules::notifications::*;

#[cfg(test)]
#[path = "../../../tests/support/notification_definitions.rs"]
mod notification_test_definitions;
