//! Filesystem schedule history: `routine-history.json` beside the app directory.
use crate::platform::error::ApiResult;
use crate::storage::stores::ScheduleStore;
use crate::storage::write_json;
use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) struct FilesystemScheduleStore {
    file: PathBuf,
}

impl FilesystemScheduleStore {
    pub(crate) fn new(file: PathBuf) -> Self {
        Self { file }
    }
}

#[async_trait]
impl ScheduleStore for FilesystemScheduleStore {
    async fn read_history(&self) -> Vec<Value> {
        match tokio::fs::read_to_string(&self.file).await {
            Ok(text) => serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|doc| doc.as_array().cloned())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    async fn write_history(&self, items: &[Value]) -> ApiResult<()> {
        write_json(&self.file, &Value::Array(items.to_vec())).await?;
        Ok(())
    }
}
