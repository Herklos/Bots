//! The anytype settings store: the full document stays local, and the shared
//! subset travels through the typed outbox. A remote read overlays the shared
//! keys onto the local document, so themes and other UI preferences never
//! leave the machine while userName, rules, sections and beta flags sync.

use super::mapper;
use super::TypedCtx;
use crate::storage::adapters::filesystem::FilesystemSettingsStore;
use crate::storage::stores::SettingsStore;
use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug)]
pub(crate) struct AnytypeSettingsStore {
    local: FilesystemSettingsStore,
    ctx: Arc<TypedCtx>,
}

impl AnytypeSettingsStore {
    pub(crate) fn new(local_path: PathBuf, ctx: Arc<TypedCtx>) -> Self {
        Self {
            local: FilesystemSettingsStore::new(local_path),
            ctx,
        }
    }

    /// The remote shared document, or `Null` when the space has none yet or
    /// the backend is unreachable this time: a read must not fail the API.
    async fn remote_props(&self) -> Value {
        let object_id = match self
            .ctx
            .find_object(mapper::SETTINGS_TYPE, mapper::SETTINGS_NATURAL)
            .await
        {
            Ok(Some(id)) => id,
            Ok(None) => return Value::Null,
            Err(error) => {
                tracing::warn!("settings read kept local values: {}", super::error_note(&error));
                return Value::Null;
            }
        };
        match self.ctx.get_object(&object_id).await {
            Ok(object) => object
                .get("properties")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
            Err(error) => {
                tracing::warn!("settings read kept local values: {}", super::error_note(&error));
                Value::Null
            }
        }
    }
}

#[async_trait]
impl SettingsStore for AnytypeSettingsStore {
    async fn read(&self) -> std::io::Result<Option<Value>> {
        let Some(mut doc) = self.local.read().await? else {
            return Ok(None);
        };
        let remote = self.remote_props().await;
        if remote.is_object() {
            mapper::apply_settings_props(&mut doc, &remote);
            // Pending local writes win: a device that just edited a shared key
            // must read back its own value until the write confirms.
            if let Some(pending) = self.ctx.pending_properties(
                mapper::SETTINGS_TYPE,
                mapper::SETTINGS_NATURAL,
            ) {
                mapper::apply_settings_props(&mut doc, &Value::Object(pending));
            }
        }
        Ok(Some(doc))
    }

    async fn write(&self, value: &Value) -> std::io::Result<()> {
        self.local.write(value).await?;
        let props = mapper::settings_props(value);
        self.ctx
            .submit(
                mapper::SETTINGS_TYPE,
                mapper::SETTINGS_NATURAL,
                &Value::Object(props),
            )
            .await;
        Ok(())
    }

    async fn initialize(&self, defaults: &Value) -> std::io::Result<()> {
        self.local.initialize(defaults).await
    }
}
