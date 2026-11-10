//! Independently compiled capability boundary.
pub mod shared {
    pub mod environment;
    pub mod file_output;
    pub mod state_changes;
    pub mod tasks;
}
pub use shared::environment::*;
pub use shared::file_output::*;
pub use shared::state_changes::*;

pub use shared::tasks;

pub use shared::{environment, file_output, state_changes};
