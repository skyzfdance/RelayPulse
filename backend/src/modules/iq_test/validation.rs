use super::{invalid, IqQuestionInput, IqStartInput, Result};
pub(super) fn identifier(id: &str, field: &'static str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return Err(invalid(field, "IQ 标识无效。"));
    }
    Ok(())
}
pub(super) fn question(mut input: IqQuestionInput) -> Result<IqQuestionInput> {
    input.content = input.content.trim().into();
    if input.content.is_empty() || input.content.chars().count() > 4096 {
        return Err(invalid("input.content", "题面须为 1–4096 个字符。"));
    }
    input.reference_answer = input
        .reference_answer
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    if input
        .reference_answer
        .as_ref()
        .is_some_and(|s| s.chars().count() > 4096)
    {
        return Err(invalid(
            "input.referenceAnswer",
            "参考答案最多 4096 个字符。",
        ));
    }
    Ok(input)
}
pub(super) fn start(input: &IqStartInput) -> Result<()> {
    if input.retry_max > 20 {
        return Err(invalid("input.retryMax", "重试次数须为 0–20 的整数。"));
    }
    let distinct: std::collections::HashSet<_> = input.question_ids.iter().collect();
    if input.question_ids.is_empty() || distinct.len() != input.question_ids.len() {
        return Err(invalid(
            "input.questionIds",
            "请至少选择一道有效题目，题目不可重复。",
        ));
    }
    for id in &input.question_ids {
        identifier(id, "input.questionIds")?;
    }
    Ok(())
}

pub(super) fn parameters(parameters: &serde_json::Map<String, serde_json::Value>) -> Result<()> {
    use serde_json::Value;
    let fail = || {
        invalid(
            "input.parameters",
            "IQ 参数含保留字段、不安全路径或无效类型。",
        )
    };
    crate::shared::model_client::validate_generation_parameters(parameters).map_err(|_| fail())?;
    if parameters.keys().any(|key| {
        matches!(
            key.to_ascii_lowercase().as_str(),
            "model" | "messages" | "prompt" | "input" | "stream" | "system"
        )
    }) {
        return Err(fail());
    }
    for (key, value) in parameters {
        let valid = match key.as_str() {
            "temperature" | "top_p" | "frequency_penalty" | "presence_penalty" => value.is_number(),
            "max_tokens" | "max_completion_tokens" | "max_output_tokens" | "top_k" | "n" => {
                value.as_u64().is_some_and(|n| n > 0)
            }
            "seed" => value.as_i64().is_some(),
            "reasoning_effort" => value.is_string(),
            "reasoning" => value
                .as_object()
                .is_some_and(|fields| fields.get("effort").is_none_or(Value::is_string)),
            "thinking" => value.as_object().is_some_and(|fields| {
                fields.get("type").is_none_or(Value::is_string)
                    && fields
                        .get("budget_tokens")
                        .is_none_or(|v| v.as_u64().is_some_and(|n| n > 0))
            }),
            "stream_options" | "response_format" | "metadata" => value.is_object(),
            "tools" => value.is_array(),
            "parallel_tool_calls" | "store" => value.is_boolean(),
            _ => true,
        };
        if !valid {
            return Err(fail());
        }
    }
    Ok(())
}
