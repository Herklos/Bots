//! Typed-object sync for the P4 stores: settings and Bot profiles push
//! through a durable outbox and read straight from the space. Memory, skills
//! and library files keep using the file-sync engine; Bot prompts, groups,
//! schedules and connector credentials still sync later (P4/P5 remainders).

pub(crate) mod bots;
pub(crate) mod hk;
pub(crate) mod mapper;
pub(crate) mod outbox;
pub(crate) mod settings;
pub(crate) mod worker;

#[cfg(test)]
mod tests;

use super::client::{AnytypeClient, AnytypeError};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// One per gateway process, shared by the typed store adapters and the worker.
/// The outbox owns pending remote writes; `drain` resolves natural keys, then
/// creates or patches holding this device's idempotency key.
pub(crate) struct TypedCtx {
    client: AnytypeClient,
    device_id: String,
    outbox: Mutex<outbox::Outbox>,
}

impl std::fmt::Debug for TypedCtx {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TypedCtx")
            .field("device_id", &self.device_id)
            .field("outbox", &self.outbox.lock().expect("typed outbox").is_empty())
            .finish_non_exhaustive()
    }
}

impl TypedCtx {
    #[cfg(test)]
    pub(crate) fn new(client: AnytypeClient, device_id: &str, outbox: outbox::Outbox) -> Self {
        Self {
            client,
            device_id: device_id.to_string(),
            outbox: Mutex::new(outbox),
        }
    }

    pub(crate) fn outbox_path(data_root: &std::path::Path) -> std::path::PathBuf {
        data_root.join("sync").join("typed-outbox.jsonl")
    }

    pub(crate) async fn get_object(&self, object_id: &str) -> Result<Value, AnytypeError> {
        self.client.get_object(object_id).await
    }

    /// Search one type, projecting the named property fields. `id` and `name`
    /// come back on every row on their own; the live app rejects them as
    /// `fields` entries with 400 unknown property keys.
    pub(crate) async fn search_type(
        &self,
        type_key: &str,
        fields: &[&str],
    ) -> Result<Vec<Value>, AnytypeError> {
        let mut body = json!({ "type": type_key });
        if !fields.is_empty() {
            body["fields"] = json!(fields);
        }
        self.client.search_strict(&body).await
    }

    /// The shared settings object, or `None` when the space has none yet.
    pub(crate) async fn settings_object(&self) -> Result<Option<Value>, AnytypeError> {
        match self.find_object(mapper::SETTINGS_TYPE, mapper::SETTINGS_NATURAL).await? {
            Some(object_id) => Ok(Some(self.client.get_object(&object_id).await?)),
            None => Ok(None),
        }
    }

    /// Read-your-writes: properties queued but not yet confirmed.
    pub(crate) fn pending_properties(
        &self,
        type_key: &str,
        natural_key: &str,
    ) -> Option<serde_json::Map<String, Value>> {
        self.outbox.lock().expect("typed outbox").merged(type_key, natural_key)
    }

    /// Queue one write and try to flush it right away. A failure that leaves
    /// the item queued is not an error for the caller: the worker retries.
    pub(crate) async fn submit(&self, type_key: &str, natural_key: &str, props: &Value) {
        {
            let mut outbox = self.outbox.lock().expect("typed outbox");
            outbox.push(
                type_key,
                natural_key,
                &props.as_object().cloned().unwrap_or_default(),
            );
        }
        self.persist().await;
        self.drain().await;
    }

    /// Serialize the queue under the lock, write the journal outside it.
    async fn persist(&self) {
        let text = {
            let outbox = self.outbox.lock().expect("typed outbox");
            outbox.journal_text()
        };
        let path = Self::outbox_path_of(&self.outbox);
        if let Err(error) = write_journal(&path, &text).await {
            tracing::warn!("typed outbox persist failed: {error}");
        }
    }

    fn outbox_path_of(outbox: &Mutex<outbox::Outbox>) -> std::path::PathBuf {
        // The path lives on the Outbox; journal_text needs no lock, so this
        // keeps only the load-time path in one place.
        outbox.lock().expect("typed outbox").path.clone()
    }

    /// Push every due pending write. Never fails: items that fail stay queued
    /// with a longer backoff, and the worker looks again.
    pub(crate) async fn drain(&self) {
        let due = {
            let outbox = self.outbox.lock().expect("typed outbox");
            outbox.due(unix_ms())
        };
        for (type_key, natural_key) in due {
            let props = {
                let outbox = self.outbox.lock().expect("typed outbox");
                outbox
                    .merged(&type_key, &natural_key)
                    .map(Value::Object)
            };
            let Some(props) = props else {
                continue;
            };
            match self.push_object(&type_key, &natural_key, &props).await {
                Ok(()) => {
                    let mut outbox = self.outbox.lock().expect("typed outbox");
                    outbox.applied(&type_key, &natural_key);
                }
                Err(error) => {
                    tracing::warn!(
                        "typed sync {type_key}/{natural_key} deferred: {}",
                        error_note(&error)
                    );
                    let mut outbox = self.outbox.lock().expect("typed outbox");
                    outbox.failed(&type_key, &natural_key);
                }
            }
            self.persist().await;
        }
    }

    /// Create or patch the object a natural key resolves to.
    async fn push_object(
        &self,
        type_key: &str,
        natural_key: &str,
        props: &Value,
    ) -> Result<(), AnytypeError> {
        let object_id = self.find_object(type_key, natural_key).await?;
        let idem = self.idem(type_key, natural_key, props, object_id.is_some());
        match object_id {
            Some(id) => {
                let body = json!({
                    "ops": [{ "op": "set_properties", "set": props }],
                });
                self.client.patch_object(&id, &body, None, &idem).await?;
            }
            None => {
                let name = object_name(type_key, natural_key, props);
                let body = json!({
                    "type": type_key,
                    "name": name,
                    "properties": props,
                });
                self.client.create_object(&body, &idem).await?;
            }
        }
        Ok(())
    }

    /// Resolve a natural key to an object id, or `None` when absent. Two
    /// settings singletons would be a conflict; the smaller id wins and the
    /// duplicate is left for import tooling to clean up.
    pub(crate) async fn find_object(
        &self,
        type_key: &str,
        natural_key: &str,
    ) -> Result<Option<String>, AnytypeError> {
        match natural_field(type_key) {
            None => Ok(None),
            // The singleton: the settings object carries no key of its own.
            Some(None) => {
                let rows = self
                    .client
                    .search_strict(&json!({ "type": type_key }))
                    .await?;
                Ok(rows
                    .iter()
                    .filter_map(|row| row["id"].as_str().map(str::to_string))
                    .min())
            }
            Some(Some(field)) => {
                let rows = self
                    .client
                    .search_strict(&json!({ "type": type_key, "fields": [field] }))
                    .await?;
                Ok(rows
                    .iter()
                    .filter(|row| {
                        let Some(props) = mapper::properties_at(row) else {
                            return false;
                        };
                        props
                            .get(field)
                            .map(|value| match value {
                                Value::Array(items) => {
                                    items.first().and_then(Value::as_str) == Some(natural_key)
                                }
                                value => value.as_str() == Some(natural_key),
                            })
                            .unwrap_or(false)
                    })
                    .filter_map(|row| row["id"].as_str().map(str::to_string))
                    .min())
            }
        }
    }

    fn idem(
        &self,
        type_key: &str,
        natural_key: &str,
        props: &Value,
        update: bool,
    ) -> String {
        let op = if update { "update" } else { "create" };
        let sha = sha256_hex(props.to_string().as_bytes());
        format!(
            "hbots-typed:{}:{}:{}:{}:{}",
            self.device_id, type_key, natural_key, op, sha
        )
    }
}

/// The property that carries a domain's natural key, or `None` for the
/// settings singleton.
pub(crate) fn natural_field(type_key: &str) -> Option<Option<&'static str>> {
    match type_key {
        mapper::SETTINGS_TYPE => Some(None),
        mapper::BOT_TYPE => Some(Some("m_slug")),
        _ => None,
    }
}

/// Everything a gateway needs once the saved backend is Anytype and the space
/// is prepared. `None` keeps the previous filesystem stores, with the reason
/// in the log.
pub(crate) struct TypedStores {
    pub(crate) settings: Arc<dyn crate::storage::stores::SettingsStore>,
    pub(crate) bots: Arc<dyn crate::storage::stores::BotStore>,
    pub(crate) ctx: Arc<TypedCtx>,
}

/// Build the typed store set, mirroring what the sync engine requires. Called
/// before the engine starts; the engine opens its own client afterwards.
pub(crate) async fn try_open(
    cfg: &crate::platform::config::Config,
) -> Option<TypedStores> {
    let stored = crate::storage::settings::read_load(&cfg.data_root).stored;
    let host = crate::storage::settings::effective_host(&cfg.storage, &stored);
    let space = crate::storage::settings::effective_space(&cfg.storage, &stored);
    if crate::storage::settings::normalize_host(&host).is_err() {
        return None;
    }
    let key = crate::storage::settings::read_key(&cfg.data_root, &cfg.storage)?;
    let client = match crate::storage::adapters::anytype::open_client(&host, &key, &space).await {
        Ok(client) => client,
        Err(error) => {
            tracing::warn!("typed sync is off: the space check failed ({})", error.message);
            return None;
        }
    };
    if !super::schema_ready(&client).await.unwrap_or(false) {
        tracing::warn!("typed sync is off: the space is not prepared for this gateway version");
        return None;
    }
    let device_id = load_device_id(&cfg.data_root.join("sync"));
    let outbox = outbox::Outbox::load(TypedCtx::outbox_path(&cfg.data_root));
    let ctx = Arc::new(TypedCtx {
        client,
        device_id: device_id.clone(),
        outbox: Mutex::new(outbox),
    });
    Some(TypedStores {
        settings: Arc::new(settings::AnytypeSettingsStore::new(
            cfg.data_root.join(".botapp/settings.json"),
            Arc::clone(&ctx),
        )),
        bots: Arc::new(bots::AnytypeBotStore::new(
            cfg.bots_root.clone(),
            cfg.data_root.clone(),
            Arc::clone(&ctx),
        )),
        ctx,
    })
}

/// The sync engine's device id, shared so both journals use one identity.
/// The first writer creates it.
fn load_device_id(sync_dir: &std::path::Path) -> String {
    let path = sync_dir.join("device-id");
    if let Ok(text) = std::fs::read_to_string(&path) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let _ = std::fs::create_dir_all(sync_dir);
    let _ = std::fs::write(&path, format!("{id}\n"));
    id
}

fn object_name(type_key: &str, natural_key: &str, props: &Value) -> String {
    if type_key == mapper::SETTINGS_TYPE {
        return "Hbots settings".to_string();
    }
    if type_key == mapper::BOT_TYPE {
        return props["m_label"]
            .as_str()
            .or_else(|| props["m_slug"].as_str())
            .unwrap_or(natural_key)
            .to_string();
    }
    natural_key.to_string()
}

/// One line for the log, the same sense the file-sync engine uses.
pub(crate) fn error_note(error: &AnytypeError) -> String {
    match error {
        AnytypeError::Unreachable => "unreachable".to_string(),
        AnytypeError::NotGranted => "not granted".to_string(),
        AnytypeError::EtagMismatch { .. } => "etag mismatch".to_string(),
        AnytypeError::RateLimited { retry_after } => {
            format!("rate limited, retry after {}s", retry_after.as_secs())
        }
        AnytypeError::ObjectArchived => "object archived".to_string(),
        AnytypeError::Gone => "gone".to_string(),
        AnytypeError::ApiChanged => "api changed".to_string(),
        AnytypeError::Bad { status, message } => format!("{status} {message}"),
    }
}

pub(crate) fn unix_ms() -> i64 {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

pub(crate) fn sleep_duration() -> Duration {
    Duration::from_secs(5)
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(bytes))
}

async fn write_journal(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    crate::storage::write_atomic(path, text.as_bytes(), None).await
}
