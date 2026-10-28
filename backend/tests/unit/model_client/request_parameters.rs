use super::*;

#[test]
fn request_payloads_and_parameter_conversion_are_independent_of_site_policies() {
    for (protocol, expected) in [
        (
            Protocol::OpenaiCompatible,
            json!({"model":"remote-name","messages":[{"role":"user","content":"只回复 OK"}],"stream":true,"max_tokens":8}),
        ),
        (
            Protocol::OpenaiResponses,
            json!({"model":"remote-name","input":"只回复 OK","stream":true,"max_output_tokens":8}),
        ),
        (
            Protocol::Anthropic,
            json!({"model":"remote-name","messages":[{"role":"user","content":"只回复 OK"}],"stream":true,"max_tokens":8}),
        ),
        (Protocol::CustomHttp, json!({"model":"remote-name"})),
    ] {
        assert_eq!(
            request::prepare(&request("https://example.test", protocol))
                .unwrap()
                .body,
            expected
        );
    }
    let mut input = request("https://example.test", Protocol::OpenaiResponses);
    input.body = CheckBody::Standard {
        parameters: serde_json::from_value(json!([
            {"key":"reasoning.effort","value":"high","type":"string"},
            {"key":"max_output_tokens","value":"64","type":"number"},
            {"key":"metadata.options","value":"{\"tags\":[1.0],\"large\":9007199254740993}","type":"json"},
            {"key":"metadata.enabled","value":"false","type":"boolean"},
            {"key":"metadata.empty","value":"","type":"string"}
        ])).unwrap() };
    let body = request::prepare(&input).unwrap().body;
    assert_eq!(body["reasoning"], json!({"effort":"high"}));
    assert_eq!(
        body["metadata"],
        json!({"options":{"tags":[1],"large":9007199254740992u64},"enabled":false,"empty":""})
    );
    assert_eq!(body["max_output_tokens"], json!(64));
}
#[test]
fn parameter_table_controls_every_body_field_and_parent_state_for_every_protocol() {
    for protocol in [
        Protocol::OpenaiCompatible,
        Protocol::OpenaiResponses,
        Protocol::Anthropic,
        Protocol::CustomHttp,
    ] {
        let mut input = request("https://example.test", protocol);
        let mut values: Vec<Parameter> = serde_json::from_value(json!([
            {"key":"reasoning.effort","value":"low","type":"string","enabled":true},
            {"key":"reasoning","value":"{}","type":"object","enabled":true},
            {"key":"input","value":"只回复 OK","type":"string","enabled":true},
            {"key":"max_tokens","value":"8","type":"number","enabled":true},
            {"key":"max_output_tokens","value":"invalid","type":"number","enabled":false}
        ]))
        .unwrap();
        values.push(Parameter::model());
        input.body = CheckBody::Standard {
            parameters: values.clone(),
        };
        assert_eq!(
            request::prepare(&input).unwrap().body,
            json!({"model":"remote-name","input":"只回复 OK","max_tokens":8,"reasoning":{"effort":"low"}})
        );
        values[0].enabled = false;
        input.body = CheckBody::Custom {
            parameters: values.clone(),
        };
        assert_eq!(
            request::prepare(&input).unwrap().body["reasoning"],
            json!({})
        );
        values[0].enabled = true;
        values[0].kind = ParameterType::Json;
        values[0].value = "{".into();
        values[1].enabled = false;
        values[5].enabled = false;
        input.body = CheckBody::Standard { parameters: values };
        assert_eq!(
            request::prepare(&input).unwrap().body,
            json!({"input":"只回复 OK","max_tokens":8})
        );
        input.body = CheckBody::Custom { parameters: vec![] };
        assert_eq!(request::prepare(&input).unwrap().body, json!({}));
    }
}
