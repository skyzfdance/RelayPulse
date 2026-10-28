use super::*;

#[tokio::test]
async fn sync_requires_actual_check_preserves_retained_models_and_rejects_stale_or_forged_plans() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::json(json!({"data":[{"id":"kept"},{"id":"added"}]})),
        Response::json(json!(["kept", "added"])),
        Response::json(json!(["kept", "added"])),
    ]);
    let id = f.site(&server.url, &["kept", "old"]);
    let before = f.store.get(&id).unwrap();
    let kept = &before.models[0];
    f.store.set_model_enabled(&id, &kept.id, false).unwrap();
    f.store
        .save_policy(
            &id,
            Some(&kept.id),
            Some(&ProbePolicy {
                parameters: serde_json::from_value(
                    json!([{ "key":"input", "value":"独立配置", "type":"string" }]),
                )
                .unwrap(),
                ..Default::default()
            }),
            None,
        )
        .unwrap();
    let baseline = f.store.get(&id).unwrap();
    let names = vec!["kept".into(), "old".into()];
    let input = ApplySync {
        models: names.clone(),
        added: vec!["added".into()],
        removed: vec![before.models[1].id.clone()],
    };
    assert!(f
        .service
        .execute(Operation::Apply(id.clone(), input.clone()))
        .await
        .is_err());
    let changes = f.execute(Operation::Sync(id.clone(), names.clone())).await;
    assert_eq!(
        changes,
        json!({"added":["added"],"removed":[{"id":before.models[1].id,"name":"old"}]})
    );
    assert_eq!(f.store.get(&id).unwrap(), baseline);
    let mut forged = input.clone();
    forged.added = vec!["not-confirmed".into()];
    assert!(f
        .service
        .execute(Operation::Apply(id.clone(), forged))
        .await
        .is_err());
    f.store.set_enabled(&id, true).unwrap();
    assert!(f
        .service
        .execute(Operation::Apply(id.clone(), input.clone()))
        .await
        .is_err());
    f.execute(Operation::Sync(id.clone(), names)).await;
    assert_eq!(
        f.execute(Operation::Apply(id.clone(), input.clone())).await,
        Value::Null
    );
    let after = f.store.get(&id).unwrap();
    assert_eq!(after.models[0], baseline.models[0]);
    assert_eq!(after.models.len(), 2);
    assert_ne!(after.models[1].id, after.models[1].name);
    assert!(f
        .service
        .execute(Operation::Apply(id.clone(), input))
        .await
        .is_err());
    assert_eq!(
        f.execute(Operation::Sync(
            id.clone(),
            vec!["kept".into(), "added".into()]
        ))
        .await,
        json!({"added":[],"removed":[]})
    );
    assert_eq!(f.store.get(&id).unwrap(), after);
    assert_eq!(server.requests().len(), 3);
}
