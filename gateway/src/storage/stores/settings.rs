//! Settings persistence interface. Remote adapters return errors for incomplete reads.
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub(crate) trait SettingsStore: std::fmt::Debug + Send + Sync {
    /// An absent document differs from an unavailable backend.
    async fn read(&self) -> std::io::Result<Option<Value>>;
    async fn write(&self, value: &Value) -> std::io::Result<()>;
    /// Initialize only if no settings exist; an unreadable document is not absent.
    async fn initialize(&self, defaults: &Value) -> std::io::Result<()>;
}
