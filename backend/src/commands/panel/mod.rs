mod error;
mod handlers;
mod window;
pub(super) use handlers::*;
pub(super) use window::*;

#[cfg(test)]
#[path = "../../../tests/integration/commands/panel.rs"]
mod tests;
