# RelayPulse

RelayPulse 是面向 macOS、Windows 与 Linux 桌面的 AI API 模型可用性探针。

## 构建与分发

RelayPulse 仅以源代码形式提供，不提供官方预构建安装包。使用者需要自行审查代码、配置构建环境并生成可执行产物。

macOS 版本不以 Mac App Store 为发布目标。当前 Tauri 构建明确启用 `macos-private-api` 和 `macOSPrivateApi`，以支持项目所需的 macOS 私有 API；由此生成的应用不符合 Mac App Store 的审核与上架要求。

- [项目总方案](./docs/PROJECT_SPEC.md)
- [诊断日志接入规范](./DIAGNOSTICS.md)
- [MVP 01：跨平台托盘探针](./docs/01-relaypulse-desktop/README.md)
- [MVP 02：后端主链路与真实状态](./docs/02-relaypulse-backend/README.md)
- [MVP 03：通知中心与模型状态推送（未来阶段）](./docs/03-notification-center/README.md)
- [MVP 04：智力天梯与页面抓取（未来阶段）](./docs/04-intelligence-ladder/README.md)
- [MVP 05：操作记录与诊断日志查询（未来阶段）](./docs/05-operation-history/README.md)
- [MVP 06：诊断接入治理与存量代码审计（未来阶段）](./docs/06-diagnostics-governance/README.md)
- [UI 定稿与成品验收基准](./docs/design/final/README.md)
