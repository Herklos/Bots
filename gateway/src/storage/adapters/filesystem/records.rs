//! Filesystem records retain the legacy library.json representation.
use crate::platform::error::{ApiError, ApiResult};
use crate::storage::stores::RecordStore;
use crate::storage::workspace::Workspace;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug)]
pub(crate) struct FilesystemRecordStore {
    ws: Arc<dyn Workspace>,
    lock: Mutex<()>,
}

impl FilesystemRecordStore {
    pub(crate) fn new(ws: Arc<dyn Workspace>) -> Self {
        Self {
            ws,
            lock: Mutex::new(()),
        }
    }
    fn path(slug: &str) -> ApiResult<String> {
        if !crate::bots::valid_slug(slug) {
            return Err(ApiError::bad_request("bad slug"));
        }
        Ok(format!("{slug}/.bot/library.json"))
    }
    async fn read(&self, slug: &str) -> ApiResult<Vec<Value>> {
        match self.ws.read(&Self::path(slug)?).await {
            Ok(bytes) => {
                let doc: Value = serde_json::from_slice(&bytes)
                    .map_err(|_| ApiError::new(500, "library.json is not valid JSON"))?;
                Ok(doc["items"].as_array().cloned().unwrap_or_default())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }
    async fn write(&self, slug: &str, items: &[Value]) -> ApiResult<()> {
        let text = serde_json::to_string_pretty(&json!({"version":1,"items":items}))
            .unwrap_or_else(|_| "{}".into())
            + "\n";
        self.ws
            .write(&Self::path(slug)?, text.as_bytes(), Some(0o600))
            .await?;
        Ok(())
    }
}

#[async_trait]
impl RecordStore for FilesystemRecordStore {
    async fn list(&self, slug: &str) -> ApiResult<Vec<Value>> {
        let _guard = self.lock.lock().await;
        self.read(slug).await
    }
    async fn put(&self, slug: &str, item: &Value, create: bool) -> ApiResult<()> {
        let _guard = self.lock.lock().await;
        let id = item["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| ApiError::bad_request("record needs an id"))?;
        let mut items = self.read(slug).await?;
        if create {
            if items.iter().any(|row| row["id"] == id) {
                return Err(ApiError::new(409, "library item already exists"));
            }
            items.push(item.clone());
        } else {
            let found = items
                .iter_mut()
                .find(|row| row["id"] == id)
                .ok_or_else(|| ApiError::not_found("no such library item"))?;
            for key in ["kind", "title", "status", "body"] {
                found[key] = item[key].clone();
            }
        }
        self.write(slug, &items).await
    }
    async fn delete(&self, slug: &str, id: &str) -> ApiResult<()> {
        let _guard = self.lock.lock().await;
        let mut items = self.read(slug).await?;
        let before = items.len();
        items.retain(|item| item["id"] != id);
        if items.len() == before {
            return Err(ApiError::not_found("no such library item"));
        }
        self.write(slug, &items).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::LocalDisk;

    #[derive(Debug, Default)]
    struct MemoryRecords(Mutex<Vec<Value>>);

    #[async_trait]
    impl RecordStore for MemoryRecords {
        async fn list(&self, _: &str) -> ApiResult<Vec<Value>> {
            Ok(self.0.lock().await.clone())
        }
        async fn put(&self, _: &str, item: &Value, _: bool) -> ApiResult<()> {
            self.0.lock().await.push(item.clone());
            Ok(())
        }
        async fn delete(&self, _: &str, id: &str) -> ApiResult<()> {
            self.0.lock().await.retain(|item| item["id"] != id);
            Ok(())
        }
    }

    #[tokio::test]
    async fn library_uses_the_injected_record_store() {
        let dir = tempfile::tempdir().unwrap();
        let ws: Arc<dyn Workspace> = Arc::new(LocalDisk::new(dir.path().into()));
        let records = Arc::new(MemoryRecords::default());
        let library = crate::bots::library::Library::with_record_store(
            ws,
            String::new(),
            String::new(),
            records.clone(),
        );
        library
            .save_item("ada", &json!({"kind":"task", "title":"Ship"}), 0)
            .await
            .unwrap();
        let rows = records.list("ada").await.unwrap();
        assert_eq!(rows[0]["title"], "Ship");
        assert!(!dir.path().join("ada/.bot/library.json").exists());
        let id = rows[0]["id"].as_str().unwrap();
        library.delete_item("ada", id).await.unwrap();
        assert!(records.list("ada").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn edits_preserve_other_fields_and_bad_documents_are_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesystemRecordStore::new(Arc::new(LocalDisk::new(dir.path().into())));
        store.put("ada", &json!({"id":"r1", "kind":"task", "title":"Old", "status":"todo", "body":"", "at":"original", "future":"kept"}), true).await.unwrap();
        store.put("ada", &json!({"id":"r1", "kind":"task", "title":"New", "status":"doing", "body":"body", "at":"changed"}), false).await.unwrap();
        let list = store.list("ada").await.unwrap();
        assert_eq!(list[0]["at"], "original");
        assert_eq!(list[0]["future"], "kept");
        assert_eq!(list[0]["title"], "New");
        let path = dir.path().join("ada/.bot/library.json");
        tokio::fs::write(&path, "{broken").await.unwrap();
        assert!(store.delete("ada", "r1").await.is_err());
        assert_eq!(tokio::fs::read_to_string(path).await.unwrap(), "{broken");
    }

    #[tokio::test]
    async fn concurrent_creates_do_not_replace_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(FilesystemRecordStore::new(Arc::new(LocalDisk::new(
            dir.path().into(),
        ))));
        let mut tasks = Vec::new();
        for index in 0..20 {
            let store = store.clone();
            tasks.push(tokio::spawn(async move {
                store
                    .put("ada", &json!({"id":format!("r{index}")}), true)
                    .await
                    .unwrap();
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(store.list("ada").await.unwrap().len(), 20);
    }
}
