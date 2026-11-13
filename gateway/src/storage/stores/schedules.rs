//! Routine schedule persistence: the short local history of changes the gateway
//! applied. Tasks themselves live in the harness; this stays on one machine.
use crate::platform::error::ApiResult;
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub(crate) trait ScheduleStore: std::fmt::Debug + Send + Sync {
    /// The local history, newest first. A missing or damaged file reads as empty,
    /// mirroring the filesystem behavior this interface preserves.
    async fn read_history(&self) -> Vec<Value>;
    /// Replace the whole local history. Errors are returned so callers can log them.
    async fn write_history(&self, items: &[Value]) -> ApiResult<()>;
}
