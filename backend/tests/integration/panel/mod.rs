use crate::panel_test_support as fixture;
mod refresh;
mod snapshots;

use super::*;
use crate::application::sites::Operation;
use crate::shared::model_client::test_support::{Response, Server};
use fixture::Fixture;
use serde_json::{json, Value};
use std::time::Duration;

async fn snapshot(f: &Fixture) -> Value {
    serde_json::to_value(f.app.panel.snapshot().await.unwrap()).unwrap()
}
