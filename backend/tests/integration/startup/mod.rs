//! 通过真实启动入口验证服务组装结果；测试按主题拆分，夹具只使用临时目录。
mod configuration;
mod failures;
mod schema;
#[path = "../../support/startup.rs"]
mod support;

mod diagnostics;
