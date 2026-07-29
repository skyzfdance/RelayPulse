# RelayPulse MVP 01：桌面壳与 UI

> 读取前置：先阅读 [RelayPulse 项目总方案](../PROJECT_SPEC.md)。项目总方案描述完整产品，本文只定义第一实施层级。
>
> UI 验收基线：[RelayPulse UI 定稿索引](../design/final/README.md)。九张 PNG 与对应 `view` 参数是本阶段最终视觉基准。
>
> 实施入口：具体技术栈、任务、测试和验收安排见 [MVP 01 开发计划](./开发计划/README.md)。

## 1. 阶段结果

先交付一个可在 macOS、Windows 和声明支持的 Linux 桌面启动的 Tauri 应用壳，并在壳内完整实现九个已经定稿的 UI 状态。主窗口能够演示多站点管理、添加与删除、站点高级设置、模型推理设置和通用设置；托盘能够打开健康与异常两种状态面板。

本阶段使用类型化 mock 数据驱动所有页面，目标是先确认 UI、交互、桌面窗口和托盘体验，不实现真实后端业务。

- 主窗口和托盘面板均运行在 Tauri 2 窗口中，不以普通网页作为最终交付物。
- 九个 `view` 都可通过固定 fixture 独立进入，便于开发、截图和验收。
- 表单、启停、拖动排序、弹窗、抽屉和页面切换具备完整前端交互反馈，但刷新应用后不承诺保留业务数据。
- 托盘图标、打开/关闭窗口、显式退出和单实例由 Rust/Tauri 壳负责。
- UI 通过 `RelayPulseDataSource` 接口读取和提交数据；当前实现为 `MockRelayPulseDataSource`，后端阶段替换 adapter，不重写页面。
- SQLite、系统凭据库、真实模型请求、调度、状态聚合和历史记录明确不在本阶段实现。

## 2. 前置依赖

- [九张 UI 定稿图](../design/final/README.md)和[可渲染原型](../design/prototype/index.html)已经确认。
- 能构建 Tauri 2 应用的 macOS 开发环境；Windows 和 Linux 可使用本机、VM 或 CI runner 做壳层验证。
- Node.js、pnpm、Rust stable 和各平台 Tauri 前置依赖；精确版本在工程初始化时写入锁文件和工具链配置。
- 本阶段不需要真实 API Key、外部模型站点、数据库或网络 mock server。

## 3. 范围内与范围外

### 3.1 范围内

- Tauri 2 + Rust + React/TypeScript + Vite 的跨平台桌面工程。
- 原生系统窗口控制、单实例、托盘常驻、显式退出、主窗口显示/隐藏和状态面板定位。
- `sites`、`add-site`、`site-advanced`、`model-settings`、`general`、`delete-confirm`、`empty` 七种主窗口状态。
- `tray-healthy`、`tray-alerts` 两种托盘面板状态，以及中性、绿、黄、红托盘图标资源。
- 多站点列表、拖动优先级、站点/模型启停、表单校验、弹窗、抽屉、删除确认和通用设置的前端交互。
- 模型推理能力、极省策略、透传参数和有效请求预览的 UI 表达与前端校验。
- 类型化领域 DTO、fixture、mock data source 和未来 Rust command/event adapter 的接口边界。
- 固定视口截图、九图视觉回归、组件测试、键盘操作和三平台壳层冒烟验证。

### 3.2 范围外

以下内容属于后端阶段，本阶段只提供对应 UI 和 mock 行为：

- SQLite schema、migration、业务配置持久化和历史记录。
- API Key 保存、系统 CredentialStore 和 Secret 生命周期。
- 模型列表发现、真实 OpenAI Compatible/Custom HTTP 请求和 SSE 解析。
- TTFT/总耗时测量、协议错误归一化和真实请求预览合并。
- 周期调度、并发控制、网络/唤醒恢复、状态机和聚合器。
- 真实开机启动设置；本阶段可保留 UI 状态，除非平台 Spike 决定顺带接通官方插件。
- 发布签名、公证、自动更新和正式发行安装包。
- 图表、复杂历史分析、云同步、账户系统和独立后台守护进程。

## 4. 数据流

```text
固定 view / 用户操作
  -> React View Model
  -> RelayPulseDataSource 接口
  -> MockRelayPulseDataSource
  -> 内存 fixture 状态
  -> React 重新渲染

Tauri tray/window event
  -> Rust shell command/event
  -> 显示、隐藏、定位或关闭对应窗口
```

- **刷新与重启**：业务 mock 状态可恢复为 fixture 初始值；不得让用户误以为数据已永久保存。
- **保存**：执行前端校验并更新内存状态，显示与未来真实保存一致的成功/失败反馈。
- **取消**：恢复打开弹窗或抽屉前的前端快照，不影响其他 fixture。
- **删除**：只从当前内存 fixture 移除，并能通过重新载入 fixture 恢复。
- **错误**：由 mock data source 注入可预测错误，验证 loading、empty、error 和 retry 状态。
- **后端接入**：后续 adapter 实现相同接口，将调用转向 Tauri typed commands/events。

## 5. 数据与权限

- 本阶段没有生产权威数据源；`MockRelayPulseDataSource` 仅用于 UI 状态和验收 fixture。
- fixture 只能使用虚构站点、模型、耗时和掩码 Key，不得包含真实凭据或第三方响应。
- API Key 输入框仅实现遮罩、显示/隐藏和前端校验，不写 SQLite、文件、日志或浏览器存储。
- 不使用 `localStorage`、IndexedDB 或 Tauri Store 冒充未来业务数据库。
- UI 不直接发起模型 API 请求；网络层、Secret 和持久化只能在后端阶段由 Rust adapter 接入。
- mock DTO 必须与未来 command/event 边界分离，后端可以替换数据源而不修改页面组件。

## 6. 模块、API 与事件契约

### 6.1 本阶段模块边界

```text
Tauri/Rust Shell
  -> application lifecycle / tray / windows / single instance
  -> 最小 shell commands/events

React UI
  -> app shell / views / feature components / design tokens
  -> RelayPulseDataSource
      -> MockRelayPulseDataSource（本阶段）
      -> TauriRelayPulseDataSource（后端阶段）
```

页面组件不得直接调用 `invoke`。只有 `TauriRelayPulseDataSource` 可以把前端接口映射到未来 Rust commands。

### 6.2 UI 数据接口

接口名称可在实现时细化，但职责保持稳定：

- `getSites()` / `saveSiteDraft()` / `reorderSites()` / `setSiteEnabled()` / `deleteSite()`
- `getModels(siteId)` / `setModelEnabled()` / `saveModelProbeDraft()`
- `getGeneralSettings()` / `saveGeneralSettingsDraft()`
- `getTraySnapshot()` / `setFixtureState()`
- `subscribeTraySnapshot(listener)`

本阶段方法返回 `Promise<Result<T, UiDataError>>` 或项目统一等价类型。mock 必须支持成功、加载、空状态和失败注入。

### 6.3 技术栈与依赖库

| 层级 | 采用技术/库 | 本阶段用途 |
|---|---|---|
| 桌面壳 | Tauri 2、Rust stable | 应用生命周期、窗口、托盘、跨平台构建 |
| Tauri 前端桥 | `@tauri-apps/api` | 调用最小壳层命令、监听窗口事件 |
| Tauri 插件 | `tauri-plugin-single-instance`、`tauri-plugin-positioner` | 单实例；托盘面板定位 |
| 可选壳层插件 | `tauri-plugin-autostart` | 仅在当前阶段决定接通真实开机启动时使用，否则后移 |
| 前端 | React、TypeScript、Vite | 组件、类型和构建 |
| UI | `antd`、`@ant-design/icons` | 设计语言、表单、表格、弹窗、抽屉和图标 |
| 拖动排序 | `@dnd-kit/react`、`@dnd-kit/helpers` | 站点优先级排序和键盘可访问拖动 |
| 状态 | React Context + `useReducer` | 页面导航、fixture 和表单草稿；当前不额外引入全局状态库 |
| 样式 | Ant Design Design Token + CSS Modules | 统一主题并精确贴合定稿图 |
| 单元/组件测试 | Vitest、Testing Library、`@testing-library/user-event` | reducer、表单、弹窗、抽屉和可访问交互 |
| E2E/视觉 | Playwright | 九个 view 截图、交互旅程和视觉差异 |
| 前端质量 | ESLint、Prettier、TypeScript `tsc` | 静态检查和格式统一 |
| Rust 质量 | `cargo fmt`、Clippy、Rust test | 壳层格式、静态检查和单元测试 |

依赖版本不在规划文档中写死小版本；工程初始化时选择当日稳定兼容版本并提交 `pnpm-lock.yaml`、`Cargo.lock` 和 Rust toolchain 配置。

本阶段不安装 `reqwest`、Tokio 网络功能、SQLx、SQLite 插件、keyring/Stronghold 或协议解析库。后端阶段根据已确认契约单独选型，避免未使用依赖扩大体积和权限。

### 6.4 UI 定稿契约

| `view` | 验收图片 | 必须表达的状态 |
|---|---|---|
| `sites` | [`sites.png`](../design/final/sites.png) | 多站点、拖动优先级、站点启停、默认探针策略和模型启用开关 |
| `add-site` | [`add-site.png`](../design/final/add-site.png) | 最小连接字段、创建后启用、连接并获取模型；遮罩覆盖左侧菜单 |
| `site-advanced` | [`site-advanced.png`](../design/final/site-advanced.png) | Endpoint、三层超时和自定义 Header 的高级入口 |
| `model-settings` | [`model-settings.png`](../design/final/model-settings.png) | 推理能力与实际策略分离、透传参数限制和有效请求预览 |
| `general` | [`general.png`](../design/final/general.png) | 启动后台、托盘行为、主题语言、历史保留和诊断日志 |
| `delete-confirm` | [`delete-confirm.png`](../design/final/delete-confirm.png) | 删除影响范围、不可撤销语义和危险操作确认；遮罩覆盖左侧菜单 |
| `empty` | [`empty.png`](../design/final/empty.png) | 首次启动无站点时只有一个主要动作 |
| `tray-healthy` | [`tray-healthy.png`](../design/final/tray-healthy.png) | 全部正常、有效站点/模型计数、最近完成和下一次探测 |
| `tray-alerts` | [`tray-alerts.png`](../design/final/tray-alerts.png) | 最多 3 个异常模型、站点名、首 Token、总耗时和异常总数入口 |

统一约束：

- 主窗口使用系统原生窗口控制，不绘制含义不明的三点或伪系统按钮。
- 弹窗保留轻阴影；抽屉只保留左边缘轻阴影。遮罩覆盖标题栏以下主内容和左侧菜单。
- 模型表第一列是“启用”开关，不是健康状态。
- 组件优先使用 Ant Design，允许用 Design Token 和局部 CSS 覆盖贴合定稿，不引入第二套 UI 组件库。

## 7. 编号任务拆分及依赖

1. **TASK-01 Tauri 跨平台桌面壳**：建立工程、窗口、托盘、单实例和 mock 状态入口；无前置任务。
2. **TASK-02 UI 基础与数据边界**：建立 Ant Design 主题、应用布局、DTO、data source、fixture 和 view 路由；依赖 TASK-01。
3. **TASK-03 主窗口 UI**：实现七种主窗口状态及完整前端交互；依赖 TASK-02。
4. **TASK-04 托盘 UI 与壳层联动**：实现两种托盘面板、状态图标、显示/隐藏和平台定位；依赖 TASK-02，可与 TASK-03 并行。
5. **TASK-05 UI 加固与候选验收**：完成组件、E2E、九图视觉、可访问性和三平台壳层冒烟；依赖 TASK-03、TASK-04。

依赖主线：`TASK-01 -> TASK-02 -> (TASK-03 + TASK-04) -> TASK-05`。

后端阶段从已验收的 `RelayPulseDataSource`、DTO、九个 view 和 Tauri 壳开始，新增持久化、Secret、探针与调度任务，不回填到本阶段完成定义。

## 8. 测试与验收

- Rust 壳层测试：单实例、窗口显示/隐藏、显式退出、托盘事件和平台降级路径。
- 前端单元/组件测试：view reducer、data source、表单校验、拖动、开关、弹窗、抽屉、loading/empty/error。
- E2E：从空状态进入站点管理，完成添加、排序、模型设置、通用设置和删除的 mock 旅程。
- 视觉：九个固定 view 均生成候选截图，并与同名定稿 PNG 人工结合 diff 审查。
- 平台冒烟：macOS、Windows 和冻结 Linux 环境可启动壳、看到托盘、打开两个窗口并显式退出。
- 不以 SQLite、真实 API、真实调度或真实耗时作为本阶段验收条件。

## 9. 技术风险与 Spike

- **BG-01 平台范围**：通过最小 Tauri 壳确认首批操作系统、WebView 和架构范围；最晚 TASK-01 退出前关闭。
- **BG-02 托盘交互**：验证三平台托盘悬停/点击事件和面板定位；不能统一时保留相同内容并采用平台触发降级。
- **BG-03 WebView 视觉差异**：用固定字体、DPI 和视口对九个 view 截图；操作系统装饰排除，应用内容必须收敛。
- **BG-04 Ant Design 适配量**：先实现 `sites`、`model-settings` 和 `tray-alerts` 三个代表 view，确认 Design Token 与局部 CSS 足以贴合定稿；不成立时只补充项目内组件，不引入第二套组件库。

## 10. 完成定义

- 三个平台的 Tauri 壳均能启动、显示托盘和窗口、关闭窗口后保持进程、显式退出。
- 九个 view 的信息结构、文案、状态、控件、遮罩和轻阴影与定稿一致。
- 所有主要交互在 mock 数据下可操作，loading、empty、error、保存和取消反馈完整。
- 页面只依赖 `RelayPulseDataSource`，没有直接调用网络、数据库、Secret 或散落的 Tauri `invoke`。
- 前端类型检查、lint、组件测试、E2E、Rust fmt/Clippy/test 和三平台构建通过。
- 九图视觉结果 9/9 有候选截图、diff/审查结论；所有证据绑定同一候选。
- 后端范围没有以临时 `localStorage`、伪请求或假持久化混入生产路径。
- 后端阶段可以在不重写 UI 的前提下实现 `TauriRelayPulseDataSource` 并替换 mock adapter。
