use super::*;
use crate::{
    modules::configuration,
    shared::persistence::{test_support::environment, Database},
};
use std::sync::{Arc, Mutex};

mod automatic;
mod persistence;
mod platform;
mod validation;

#[path = "../../support/notifications.rs"]
mod support;

pub(crate) use support::FakePlatform;
use support::{custom, service, DEV, FAILED, RECOVERED};
