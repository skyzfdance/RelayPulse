# 诊断日志接入规范

RelayPulse 会自动提供运行时兜底日志，但新功能仍需要声明业务语义。目标不是记录所有数据，而是在不泄露用户数据的前提下，通过 `actionId`、`operationId`、`revision` 和错误编号还原故障链路。

## 自动接管范围

以下能力由全局诊断层自动处理，新页面不需要重复实现：

- React 渲染错误、`window.error` 和未处理的 Promise rejection。
- Rust panic、启动失败、非正常退出和正常退出。
- 按钮点击、字段聚焦和表单提交等通用交互，只记录脱敏后的控件指纹，不记录文本和值。
- 通过 `TauriRelayPulseDataSource.mutate` 或 `observedAction` 发起的操作自动生成 `operationId`，并与前端最后一次动作关联。
- 通过 Rust `observed_user_command` 执行的命令自动记录开始、耗时、成功或标准化错误码。
- 调用 `changed` 发布的数据变化自动记录 revision 和受影响的领域。

自动兜底只能说明“用户操作了某个控件”或“某条命令失败”。它无法自动判断新功能的业务对象、变更类型和字段范围。

## 新功能接入清单

### 1. 前端语义动作

关键交互调用 `recordFrontendAction`。如果是新动作名称，需要同时加入：

- `src/app/frontend-diagnostics.ts` 的 `FrontendAction` 联合类型。
- `src-tauri/src/commands/mod.rs` 的 `validate_frontend_action` 白名单。

只记录动作、页面、对象类型和安全 ID，不记录输入值、搜索词、名称、URL 或第三方返回内容。

```ts
recordFrontendAction('setting-changed', {
  targetType: 'form',
  targetId: 'setting-example-limit',
});
```

### 2. Tauri DataSource

用户触发的写操作必须通过统一方法产生 `operationId`：

```ts
updateExample(input: ExampleInput): Promise<AppSnapshot> {
  return this.mutate('update_example', { input });
}

runExampleAction(input: ExampleInput): Promise<ExampleResult> {
  return this.observedAction('run_example_action', { input });
}
```

- 返回快照的状态变更使用 `mutate`。
- 其他用户动作使用 `observedAction`。
- 纯读取可以使用 `action`。
- 不要在功能代码中直接调用裸 `invoke` 执行用户写操作。

### 3. Rust 命令

写命令接收 `operation_id`，并使用 `observed_user_command` 包装。`OperationAudit` 只描述变更，不携带变更值：

```rust
observed_user_command(
    state.diagnostics().cloned(),
    "update-example",
    operation_id,
    Some(OperationAudit {
        target_type: "settings",
        target_id: None,
        parent_id: None,
        change_kind: "update",
        field_scope: &["exampleLimit"],
    }),
    |operation_id| async move {
        let result = service(&state)?.update_example(input).await?;
        changed(&app, &result, Some(&operation_id), &["settings"])
    },
)
.await
```

新增审计对象、变更类型或字段时，需要同步扩展 `src-tauri/src/diagnostics.rs` 中对应的安全白名单。未知字段会被诊断包过滤。

### 4. 专用事件

只有通用的操作开始、完成、数据 revision 和标准错误码不足以定位问题时，才新增专用日志事件。专用事件必须：

- 使用结构化 JSON 和固定事件名。
- 只接受枚举、布尔值、数量、耗时、安全 ID 和标准错误码。
- 在 `sanitize_log_event` 中显式声明诊断包允许导出的字段。
- 不记录原始错误正文、请求/响应正文、Header、凭据、完整 URL、用户输入或业务名称。

## 测试要求

每个新增写功能至少验证：

- 成功和失败都包含同一个 `operationId`，失败返回可搜索的错误编号。
- 数据变化包含 operation、revision 和受影响领域。
- 审计事件只有对象、变更类型和字段名，没有旧值或新值。
- 在输入中放入唯一敏感标记后，日志和诊断包中均搜索不到该标记。
- React 或后端异常发生时，可以从最后一次语义动作追到命令开始、完成或错误。

代码评审时，把“是否走统一诊断链路”作为新增用户功能的固定检查项。纯展示组件无需添加业务事件，全局交互日志已经覆盖其基础轨迹。
