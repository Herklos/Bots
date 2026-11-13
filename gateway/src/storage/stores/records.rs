//! Per-record persistence operations.
use crate::platform::error::ApiResult;
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub(crate) trait RecordStore: std::fmt::Debug + Send + Sync {
    async fn list(&self, slug: &str) -> ApiResult<Vec<Value>>;
    /// Creates a record, or changes the four editable fields of an existing one.
    async fn put(&self, slug: &str, item: &Value, create: bool) -> ApiResult<()>;
    async fn delete(&self, slug: &str, id: &str) -> ApiResult<()>;
}
