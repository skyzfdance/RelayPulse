//! 所有夹具都由测试调用方传入临时目录；不解析真实用户目录。
use crate::shared::environment::EnvironmentContext;
use std::path::Path;

pub(crate) fn environment(base: &Path, identifier: &str) -> EnvironmentContext {
    EnvironmentContext::resolve(identifier, base, &base.join(identifier)).unwrap()
}
