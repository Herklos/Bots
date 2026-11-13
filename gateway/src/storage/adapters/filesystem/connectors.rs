//! Filesystem connector documents are local and may contain credentials.
use crate::platform::error::{ApiError, ApiResult};
use crate::storage::stores::{ConnectorStore, EmptyDocument};
use crate::storage::workspace::Workspace;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug)]
pub(crate) struct FilesystemConnectorStore {
    ws: Arc<dyn Workspace>,
    lock: Mutex<()>,
}

impl FilesystemConnectorStore {
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
        Ok(format!("{slug}/.bot/connectors.json"))
    }
    async fn read(&self, slug: &str, empty: EmptyDocument) -> ApiResult<Vec<Value>> {
        match self.ws.read(&Self::path(slug)?).await {
            Ok(bytes)
                if matches!(empty, EmptyDocument::Allow)
                    && std::str::from_utf8(&bytes).is_ok_and(|text| text.trim().is_empty()) =>
            {
                Ok(Vec::new())
            }
            Ok(bytes) => {
                let doc: Value = serde_json::from_slice(&bytes)
                    .map_err(|_| ApiError::new(500, "connectors.json is not valid JSON"))?;
                Ok(doc["connectors"].as_array().cloned().unwrap_or_default())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }
    async fn write(&self, slug: &str, rows: &[Value]) -> ApiResult<()> {
        let text = serde_json::to_string_pretty(&json!({"connectors": rows}))
            .expect("JSON values serialize")
            + "\n";
        self.ws
            .write(&Self::path(slug)?, text.as_bytes(), Some(0o600))
            .await?;
        Ok(())
    }
}

#[async_trait]
impl ConnectorStore for FilesystemConnectorStore {
    async fn list(&self, slug: &str, empty: EmptyDocument) -> ApiResult<Vec<Value>> {
        let _guard = self.lock.lock().await;
        self.read(slug, empty).await
    }
    async fn put(
        &self,
        slug: &str,
        name: &str,
        url: &str,
        enabled: bool,
        secret: Option<&str>,
    ) -> ApiResult<Vec<Value>> {
        let _guard = self.lock.lock().await;
        let mut rows = self.read(slug, EmptyDocument::Reject).await?;
        if let Some(found) = rows.iter_mut().find(|row| row["name"] == name) {
            found["url"] = json!(url);
            found["enabled"] = json!(enabled);
            if let Some(secret) = secret.filter(|s| !s.is_empty()) {
                found["secret"] = json!(secret);
            }
        } else {
            rows.push(
                json!({"name":name, "url":url, "enabled":enabled, "secret":secret.unwrap_or("")}),
            );
        }
        self.write(slug, &rows).await?;
        Ok(rows)
    }
    async fn delete(&self, slug: &str, name: &str) -> ApiResult<Vec<Value>> {
        let _guard = self.lock.lock().await;
        let mut rows = self.read(slug, EmptyDocument::Reject).await?;
        let before = rows.len();
        rows.retain(|row| row["name"] != name);
        if rows.len() == before {
            return Err(ApiError::not_found("no such connector"));
        }
        self.write(slug, &rows).await?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::LocalDisk;

    #[derive(Debug, Default)]
    struct InjectedConnectorStore;

    #[async_trait]
    impl ConnectorStore for InjectedConnectorStore {
        async fn list(&self, _: &str, _: EmptyDocument) -> ApiResult<Vec<Value>> {
            Ok(vec![
                json!({"name":"notes", "url":"https://example.com", "enabled":true, "secret":"local-secret"}),
            ])
        }
        async fn put(
            &self,
            _: &str,
            _: &str,
            _: &str,
            _: bool,
            _: Option<&str>,
        ) -> ApiResult<Vec<Value>> {
            Err(ApiError::new(500, "unused in this test"))
        }
        async fn delete(&self, _: &str, _: &str) -> ApiResult<Vec<Value>> {
            Err(ApiError::new(500, "unused in this test"))
        }
    }

    #[tokio::test]
    async fn alternative_store_drives_both_bot_and_library_views() {
        let dir = tempfile::tempdir().unwrap();
        let ws: Arc<dyn Workspace> = Arc::new(LocalDisk::new(dir.path().into()));
        let connectors: Arc<dyn ConnectorStore> = Arc::new(InjectedConnectorStore);
        let bots = crate::bots::Bots::with_workspace(
            dir.path().into(),
            String::new(),
            String::new(),
            Arc::clone(&ws),
        )
        .with_connector_store(Arc::clone(&connectors));
        let library = crate::bots::library::Library::new(ws, String::new(), String::new())
            .with_connector_store(connectors);
        bots.create(&json!({"name":"Ada"})).await.unwrap();
        let visible = library.public_connectors("ada").await.unwrap();
        assert_eq!(visible[1]["name"], "notes");
        assert!(visible[1].get("secret").is_none());
        let mcp: Value = serde_json::from_slice(
            &tokio::fs::read(dir.path().join("ada/.mcp.json"))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            mcp["mcpServers"]["notes"]["headers"]["Authorization"],
            "Bearer local-secret"
        );
        assert!(!dir.path().join("ada/.bot/connectors.json").exists());
    }

    #[tokio::test]
    async fn local_secret_is_preserved_and_malformed_data_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesystemConnectorStore::new(Arc::new(LocalDisk::new(dir.path().into())));
        store
            .put(
                "ada",
                "notes",
                "https://example.com",
                true,
                Some("local-secret"),
            )
            .await
            .unwrap();
        let rows = store
            .put("ada", "notes", "https://example.org", false, None)
            .await
            .unwrap();
        assert_eq!(rows[0]["secret"], "local-secret");
        let path = dir.path().join("ada/.bot/connectors.json");
        tokio::fs::write(&path, "{broken").await.unwrap();
        assert!(store.delete("ada", "notes").await.is_err());
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), "{broken");
        tokio::fs::write(&path, "  \n").await.unwrap();
        assert!(store.list("ada", EmptyDocument::Reject).await.is_err());
        assert!(store
            .list("ada", EmptyDocument::Allow)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn library_and_bot_instructions_share_connector_store_without_exposing_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let ws: Arc<dyn Workspace> = Arc::new(LocalDisk::new(dir.path().into()));
        let connectors = Arc::new(FilesystemConnectorStore::new(Arc::clone(&ws)));
        let bots = crate::bots::Bots::with_workspace(
            dir.path().into(),
            String::new(),
            String::new(),
            Arc::clone(&ws),
        )
        .with_connector_store(connectors.clone());
        let library = crate::bots::library::Library::new(ws, String::new(), String::new())
            .with_connector_store(connectors);
        let bot = bots.create(&json!({"name":"Ada"})).await.unwrap();
        library
            .save_connector(
                "ada",
                &json!({"name":"notes", "url":"https://example.com", "secret":"local-secret"}),
            )
            .await
            .unwrap();
        bots.write_instructions(&bot).await.unwrap();
        let visible = library.public_connectors("ada").await.unwrap();
        let row = visible.iter().find(|row| row["name"] == "notes").unwrap();
        assert_eq!(row["hasSecret"], true);
        assert!(row.get("secret").is_none());
        let mcp: Value = serde_json::from_slice(
            &tokio::fs::read(dir.path().join("ada/.mcp.json"))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            mcp["mcpServers"]["notes"]["headers"]["Authorization"],
            "Bearer local-secret"
        );
    }
}
