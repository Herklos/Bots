//! Filesystem rooms: `groups.json`, with `groups.local.json` beside it when
//! local overlays are enabled and a `groups.lock` for cross-process writes.
use super::rooms_overlay;
use crate::platform::error::{ApiError, ApiResult};
use crate::storage::stores::RoomStore;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) struct FilesystemRoomStore {
    path: PathBuf,
    split_local: bool,
}

impl FilesystemRoomStore {
    pub(crate) fn new(path: PathBuf, split_local: bool) -> Self {
        Self { path, split_local }
    }
}

#[async_trait]
impl RoomStore for FilesystemRoomStore {
    async fn acquire(&self) -> std::io::Result<Option<std::fs::File>> {
        crate::storage::local_lock::acquire_if_active(
            self.path.with_file_name("groups.lock"),
            self.split_local,
        )
        .await
    }

    async fn read(&self) -> ApiResult<Value> {
        let doc = match tokio::fs::read_to_string(&self.path).await {
            Ok(text) => {
                #[allow(
                    clippy::map_err_ignore,
                    reason = "serde quotes groups.json, which holds room messages"
                )]
                let doc: Value = serde_json::from_str(&text)
                    .map_err(|_| ApiError::new(500, "groups.json is not valid"))?;
                if !doc["rooms"].is_array() {
                    return Err(ApiError::new(500, "groups.json is not valid"));
                }
                Ok::<Value, ApiError>(doc)
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Ok(json!({ "version": 1, "rooms": [] }))
            }
            Err(err) => Err(err.into()),
        }?;
        rooms_overlay::load(&self.path, doc, self.split_local).await
    }

    async fn write(&self, doc: &Value) -> ApiResult<()> {
        rooms_overlay::save(&self.path, doc, self.split_local).await
    }
}
