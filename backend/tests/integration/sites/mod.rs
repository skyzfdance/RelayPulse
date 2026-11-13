mod capture;
mod record_metadata;
mod records;
mod scheduling;
use super::*;
use crate::{
    application::lifecycle::Lifecycle,
    modules::{
        configuration::{self, ConfigurationService, SqliteSettingsStore},
        monitoring::{self, Health, Monitoring},
        notifications,
        sites::{
            self, ApplySync, CreateSite, CredentialAction, CustomRequest, ProbePolicy, Protocol,
            SiteAccess, SiteDraft, SiteMutation, SiteStore, TaskError,
        },
    },
    shared::{
        diagnostics::Diagnostics,
        model_client::{
            test_support::{Response, Server},
            ModelClient,
        },
        persistence::{test_support::environment, Database},
    },
};
use serde_json::{json, Value};
use std::time::Duration;

#[path = "../../support/site_fixture.rs"]
mod fixture;
pub(crate) use fixture::Fixture;

mod concurrency;
mod creation;
mod model_sync;
mod probe_results;
mod request_policies;

mod operation_records;

mod operation_snapshots;
