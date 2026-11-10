#![allow(dead_code)]
use crate::modules::notifications::*;
use crate::shared::persistence::{test_support::environment, Database};
use std::sync::{Arc, Mutex};
#[path = "notifications.rs"]
mod platform;
pub(crate) use platform::FakePlatform;
