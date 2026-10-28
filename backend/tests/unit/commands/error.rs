use super::*;
use serde_json::json;

#[test]
fn public_error_serialization_contract() {
    assert_eq!(
        serde_json::to_value(CommandError::invalid("fieldName", "可读说明")).unwrap(),
        json!({"code":"invalid_input", "category":"validation", "message":"可读说明", "field":"fieldName", "retryable":false, "operationId":null})
    );
    let error = CommandError {
        code: "example",
        category: "example",
        message: "可读说明".into(),
        field: None,
        retryable: true,
        operation_id: Some("operation-123".into()),
    };
    assert_eq!(
        serde_json::to_value(error).unwrap(),
        json!({"code":"example", "category":"example", "message":"可读说明", "field":null, "retryable":true, "operationId":"operation-123"})
    );
}
