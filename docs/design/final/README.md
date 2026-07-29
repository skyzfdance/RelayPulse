# RelayPulse UI 定稿索引

本目录是 RelayPulse MVP 的最终 UI 验收基线。后续产品实现以这里的 9 张 PNG 为最终成品的视觉与交互状态验收依据；旧版草稿已删除，不再参与评审。

`view` 参数、图片文件名和页面状态必须保持一一对应。除非重新确认设计，不应单独更改命名、页面结构、控件位置或状态语义。

## 界面清单

| 页面 | `view` 参数 | 验收图片 | 实现要点 |
| --- | --- | --- | --- |
| 站点管理 | `view=sites` | [`sites.png`](./sites.png) | 左侧多站点列表支持拖动优先级和启停；右侧编辑站点、默认探针策略和模型 |
| 添加站点 | `view=add-site` | [`add-site.png`](./add-site.png) | 只收集连接所需的最小字段，连接成功后再选择模型 |
| 站点高级设置 | `view=site-advanced` | [`site-advanced.png`](./site-advanced.png) | 非默认端点、超时和自定义请求头集中在高级区域 |
| 模型推理与探针设置 | `view=model-settings` | [`model-settings.png`](./model-settings.png) | 区分推理能力和实际探针策略；支持透传参数与有效请求预览 |
| 通用设置 | `view=general` | [`general.png`](./general.png) | 仅包含应用级系统行为、外观语言和本地数据选项 |
| 删除站点确认 | `view=delete-confirm` | [`delete-confirm.png`](./delete-confirm.png) | 明确受影响的模型和记录，并使用危险操作语义 |
| 首次启动空状态 | `view=empty` | [`empty.png`](./empty.png) | 单一主操作，引导添加第一个站点 |
| 托盘面板：全部正常 | `view=tray-healthy` | [`tray-healthy.png`](./tray-healthy.png) | 只显示总体状态、参与探测的站点/模型数量与更新时间 |
| 托盘面板：存在异常 | `view=tray-alerts` | [`tray-alerts.png`](./tray-alerts.png) | 最多显示 3 个异常模型；按站点优先级排序；同时展示首 Token 和总耗时 |

## 统一规则

- 以 Ant Design 的企业工具风格为基础，使用克制的中性色和语义状态色。
- 主色 `#1677FF`，成功 `#52C41A`，警告 `#FAAD14`，错误 `#FF4D4F`。
- 基础圆角为 6px，常用控件高度为 32px，间距以 8px 为主要节奏。
- 站点只有在“站点已启用”且“至少一个模型已启用”时才参与探测。
- 状态栏小面板不承担配置任务，所有复杂设置都进入主窗口。
- 模态弹窗和抽屉打开时，遮罩覆盖标题栏以下的整个应用区域，包括左侧菜单；背景不可操作。
- 图片中的应用内容参与验收，macOS、Windows 和 Linux 的系统原生窗口控制按钮不在统一视觉验收范围内。

## 可渲染原型

源文件位于 `../prototype/index.html`。本地预览服务启动后，通过查询参数 `view` 切换界面：

```text
?view=tray-healthy
?view=tray-alerts
?view=sites
?view=add-site
?view=site-advanced
?view=model-settings
?view=general
?view=delete-confirm
?view=empty
```
