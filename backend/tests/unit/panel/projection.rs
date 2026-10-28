use super::{projection::project, types::*};
use crate::modules::{
    configuration::ThemeMode,
    monitoring::{Health, HealthState, ProbeActivity},
    sites::{Connection, Model, ProbePolicy, Protocol, Site},
};
use std::collections::HashMap;

fn site(id: &str, models: &[&str]) -> Site {
    Site {
        id: id.into(),
        name: "相同名称".into(),
        protocol: Protocol::CustomHttp,
        base_url: "http://localhost".into(),
        enabled: true,
        credential_configured: true,
        connection: Connection::for_protocol(Protocol::CustomHttp),
        policy: ProbePolicy {
            version: 2,
            parameters: vec![],
        },
        models: models
            .iter()
            .map(|id| Model {
                id: (*id).into(),
                name: format!("model-{id}"),
                enabled: true,
                created_at_ms: 1000,
                policy: None,
                custom_request: None,
            })
            .collect(),
        revision: 0,
        priority: 0,
    }
}

#[test]
fn projection_keeps_ids_order_totals_threshold_conclusions_nulls_and_real_zeroes() {
    let first = site("first", &["a", "b", "pending"]);
    let mut second = site("second", &["c", "disabled"]);
    second.models[1].enabled = false;
    let mut hidden = site("hidden", &["hidden-model"]);
    hidden.enabled = false;
    let mut states = HashMap::new();
    for (id, health) in [
        ("a", Health::Healthy),
        ("b", Health::Warning),
        ("c", Health::Error),
        ("pending", Health::Muted),
        ("disabled", Health::Error),
        ("hidden-model", Health::Error),
    ] {
        states.insert(
            id.into(),
            HealthState {
                health,
                first_token_ms: Some(0),
                duration_ms: None,
                finished_at_ms: 1000,
                ..Default::default()
            },
        );
    }
    let snapshot = project(
        42,
        &[first, second, site("empty", &[]), hidden],
        &states,
        &ProbeActivity::default(),
        false,
        ThemeMode::Dark,
    )
    .unwrap();
    assert_eq!(
        (snapshot.revision, snapshot.site_count, snapshot.model_count),
        (42, 3, 4)
    );
    assert_eq!(snapshot.state, PanelState::Ready);
    assert_eq!(
        snapshot
            .sites
            .iter()
            .map(|site| site.site_id.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert_eq!(
        snapshot.sites[0]
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "pending"]
    );
    assert_eq!(
        snapshot.sites[0].models[0].status,
        PanelModelStatus::Healthy
    );
    assert_eq!(snapshot.sites[0].models[1].status, PanelModelStatus::Slow);
    assert_eq!(
        snapshot.sites[0].models[2].status,
        PanelModelStatus::Pending
    );
    assert_eq!(
        snapshot.sites[1].models[0].status,
        PanelModelStatus::Unavailable
    );
    let json = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(json["theme"], "dark");
    assert_eq!(json["sites"][0]["models"][0]["firstTokenMs"], 0);
    assert!(json["sites"][0]["models"][0]["totalMs"].is_null());
    assert_eq!(json["lastProbe"]["completedAt"], "1970-01-01T00:00:01Z");
    assert_eq!(json["lastProbe"]["completedCount"], 1);
    assert!(json.get("model_count").is_none());
}

#[test]
fn waiting_and_schedule_use_enabled_scope_and_the_same_due_formula_as_monitoring() {
    let sites = vec![site("s", &["m"])];
    let snapshot = project(
        0,
        &sites,
        &HashMap::new(),
        &ProbeActivity::default(),
        true,
        ThemeMode::System,
    )
    .unwrap();
    assert_eq!(snapshot.state, PanelState::Waiting);
    assert_eq!(snapshot.site_count, 1);
    assert_eq!(snapshot.model_count, 1);
    assert_eq!(
        snapshot.schedule,
        PanelSchedule::Scheduled {
            next_probe_at: "1970-01-01T00:05:01Z".into()
        }
    );
    assert_eq!(snapshot.last_probe, None);
    assert_eq!(snapshot.active_probe, None);
    assert_eq!(
        project(
            0,
            &sites,
            &HashMap::new(),
            &ProbeActivity::default(),
            false,
            ThemeMode::System
        )
        .unwrap()
        .schedule,
        PanelSchedule::Disabled
    );
    assert_eq!(
        project(
            0,
            &[site("empty", &[])],
            &HashMap::new(),
            &ProbeActivity::default(),
            true,
            ThemeMode::System,
        )
        .unwrap()
        .schedule,
        PanelSchedule::Disabled
    );
}

#[test]
fn all_healthy_uses_ready_without_inventing_missing_timings() {
    let states = HashMap::from([(
        "m".into(),
        HealthState {
            health: Health::Healthy,
            ..Default::default()
        },
    )]);
    let snapshot = project(
        0,
        &[site("s", &["m"])],
        &states,
        &ProbeActivity::default(),
        false,
        ThemeMode::System,
    )
    .unwrap();
    assert_eq!(snapshot.state, PanelState::Ready);
    assert_eq!(snapshot.sites[0].models[0].first_token_ms, None);
    assert_eq!(snapshot.sites[0].models[0].total_ms, None);
}

#[test]
fn completed_muted_results_are_visible_but_unprobed_models_do_not_invent_results() {
    let sites = vec![site("s", &["completed", "unprobed", "no-state"])];
    let mut states = HashMap::from([
        ("unprobed".into(), HealthState::default()),
        (
            "completed".into(),
            HealthState {
                health: Health::Muted,
                duration_ms: Some(22596),
                failures: 2,
                finished_at_ms: 1000,
                ..Default::default()
            },
        ),
    ]);
    let result = project(
        0,
        &sites,
        &states,
        &ProbeActivity::default(),
        false,
        ThemeMode::System,
    )
    .unwrap();
    assert_eq!(result.state, PanelState::Ready);
    assert_eq!((result.site_count, result.model_count), (1, 3));
    assert_eq!(result.sites[0].models.len(), 1);
    let model = &result.sites[0].models[0];
    assert_eq!(model.id, "completed");
    assert_eq!(model.status, PanelModelStatus::Pending);
    assert_eq!(model.first_token_ms, None);
    assert_eq!(model.total_ms, Some(22596));
    assert_eq!(serde_json::to_value(model).unwrap()["status"], "pending");

    states.remove("completed");
    let waiting = project(
        0,
        &sites,
        &states,
        &ProbeActivity::default(),
        false,
        ThemeMode::System,
    )
    .unwrap();
    assert_eq!(waiting.state, PanelState::Waiting);
    assert!(waiting.sites.is_empty());
    assert!(waiting.last_probe.is_none());
}
