//! Filesystem settings retain the existing malformed-document fallback.
use crate::storage::stores::SettingsStore;
use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) struct FilesystemSettingsStore {
    path: PathBuf,
}

impl FilesystemSettingsStore {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

#[async_trait]
impl SettingsStore for FilesystemSettingsStore {
    async fn read(&self) -> std::io::Result<Option<Value>> {
        Ok(crate::storage::read_json(&self.path).await)
    }
    async fn write(&self, value: &Value) -> std::io::Result<()> {
        crate::storage::write_json(&self.path, value).await
    }
    async fn initialize(&self, defaults: &Value) -> std::io::Result<()> {
        if !self.path.try_exists()? {
            self.write(defaults).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn filesystem_round_trip_and_unreadable_document_behavior() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let store: Box<dyn SettingsStore> = Box::new(FilesystemSettingsStore::new(path.clone()));
        assert_eq!(store.read().await.unwrap(), None);
        let value = json!({"userName":"Ada", "theme":"dark", "future":{"x": 1}});
        store.write(&value).await.unwrap();
        assert_eq!(store.read().await.unwrap(), Some(value));
        tokio::fs::write(&path, b"{").await.unwrap();
        assert_eq!(store.read().await.unwrap(), None);
        store
            .initialize(&json!({"userName":"Default"}))
            .await
            .unwrap();
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), "{");
    }
}
