//! Filesystem machine-local state: one JSON file per document name in the
//! adapter's directory. Atomic writes; malformed documents are refused on read.
use crate::platform::error::ApiResult;
use crate::storage::stores::LocalStateStore;
use crate::storage::write_json;
use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) struct FilesystemLocalStateStore {
    dir: PathBuf,
}

impl FilesystemLocalStateStore {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, name: &str) -> std::io::Result<PathBuf> {
        // The charset plus a hard ".." ban keeps `join` inside the directory:
        // "..".split('.') is all-empty parts, so a substring check is the reliable one.
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
            || name.contains("..")
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "bad machine-local state document name",
            ));
        }
        Ok(self.dir.join(name))
    }
}

#[async_trait]
impl LocalStateStore for FilesystemLocalStateStore {
    async fn read(&self, name: &str) -> std::io::Result<Option<Value>> {
        match tokio::fs::read_to_string(self.path(name)?).await {
            Ok(text) => serde_json::from_str::<Value>(&text)
                .map(Some)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn write(&self, name: &str, value: &Value) -> ApiResult<()> {
        write_json(&self.path(name)?, value).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn keyed_read_write_round_trips_and_absent_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesystemLocalStateStore::new(dir.path().to_owned());
        assert_eq!(store.read("thread-meta.json").await.unwrap(), None);
        store
            .write("thread-meta.json", &json!({"t": {"pins": []}}))
            .await
            .unwrap();
        assert_eq!(
            store.read("thread-meta.json").await.unwrap(),
            Some(json!({"t": {"pins": []}}))
        );
        let text = tokio::fs::read_to_string(dir.path().join("thread-meta.json"))
            .await
            .unwrap();
        assert!(text.ends_with('\n'));
    }

    #[tokio::test]
    async fn refuses_names_that_escape_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesystemLocalStateStore::new(dir.path().to_owned());
        for name in ["", "..", "../x", "a/b", "a\\b", "spaces in", "a..b"] {
            assert!(store.read(name).await.is_err(), "read of {name} accepted");
            assert!(
                store.write(name, &json!({})).await.is_err(),
                "write of {name} accepted"
            );
        }
        assert!(store.read("../secrets").await.is_err());
        assert!(!dir.path().join("secrets").exists());
    }

    #[tokio::test]
    async fn a_malformed_document_is_an_error_so_a_save_cannot_overwrite_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesystemLocalStateStore::new(dir.path().to_owned());
        tokio::fs::write(dir.path().join("state.json"), b"{").await.unwrap();
        assert!(store.read("state.json").await.is_err());
    }
}
