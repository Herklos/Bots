//! Machine-local runtime state documents that never leave this device: thread
//! reactions, pins, watch progress. `name` is one JSON file inside the adapter's
//! directory; the charset is validated so the mapping stays airtight.
use crate::platform::error::ApiResult;
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub(crate) trait LocalStateStore: std::fmt::Debug + Send + Sync {
    /// `None` when the document is absent. A malformed document is an error so a
    /// later save cannot overwrite the person's state.
    async fn read(&self, name: &str) -> std::io::Result<Option<Value>>;
    async fn write(&self, name: &str, value: &Value) -> ApiResult<()>;
}
