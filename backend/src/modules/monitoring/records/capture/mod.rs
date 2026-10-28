//! 正式监测的可读过程文件适配器。
mod body;
mod reader;
#[cfg(test)]
#[path = "../../../../../tests/unit/monitoring/capture.rs"]
mod tests;
mod text;
mod writer;

pub(in crate::modules::monitoring) use reader::CapturedRecord;
use writer::write_event;
pub(in crate::modules::monitoring) use writer::CaptureStore;
