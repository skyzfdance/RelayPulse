//! 应用组装、站点跨模块用例与生命周期；旧业务参考代码不参与编译。
pub(crate) mod composition;
pub(crate) mod diagnostics;

pub(crate) mod lifecycle;
pub(crate) mod notifications;
pub(crate) mod sites;
pub(crate) mod snapshots;

pub(crate) mod action_definitions;
pub(crate) mod operation_history;
pub(crate) mod startup_history;

pub(crate) mod environment;

pub(crate) mod data_transfer;
