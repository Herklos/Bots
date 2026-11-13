//! Room (group) document persistence: shareable room metadata and messages, with
//! the device-local runtime read and written alongside. The returned document is
//! the merged view; a write splits device-local fields away from the shared part.
use crate::platform::error::ApiResult;
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub(crate) trait RoomStore: std::fmt::Debug + Send + Sync {
    /// Hold while a read-modify-write of the room document spans several calls.
    async fn acquire(&self) -> std::io::Result<Option<std::fs::File>>;
    /// The full room document after merging the device-local overlay. A missing
    /// document is an empty version-1 doc; an unreadable one is an error.
    async fn read(&self) -> ApiResult<Value>;
    /// Write the full room document, moving device-local fields to their local store.
    async fn write(&self, doc: &Value) -> ApiResult<()>;
}
