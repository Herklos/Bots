//! Local usage counts in `bots/.botapp/usage.json`.
//!
//! The gateway counts what it did (messages, attachments, vault writes, icon fetches) and
//! keeps that on disk. Nothing is sent anywhere. A remote telemetry destination is not
//! in this repo.

use crate::platform::error::{ApiError, ApiResult};
use crate::AppState;
use axum::{extract::State, Json};
use serde_json::{json, Map, Value};
use std::{path::PathBuf, sync::Arc, time::SystemTime};
use tokio::sync::Mutex;

mod report;
#[cfg(test)]
mod tests;
mod tokens;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use report::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use tokens::*;

const KEEP_DAYS: usize = 400;

#[derive(Clone, Copy, Debug)]
pub enum Event {
    Message,
    Attachment,
    VaultWrite,
    IconFetch,
}

impl Event {
    fn key(self) -> &'static str {
        match self {
            Event::Message => "messages",
            Event::Attachment => "attachments",
            Event::VaultWrite => "vaultWrites",
            Event::IconFetch => "iconFetches",
        }
    }
}

#[derive(Debug)]
pub struct Usage {
    path: PathBuf,
    lock: Mutex<()>,
    /// Each Bot thread's turns, kept for as long as its `updatedAt` stamp holds.
    turns: Mutex<std::collections::HashMap<String, (String, Vec<Turn>)>>,
}

impl Usage {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
            turns: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Add one event. A broken file is left untouched.
    pub async fn note(&self, event: Event) {
        self.note_for("", event).await;
    }

    /// Count `event` for one Bot and for the machine. An empty slug is machine-only.
    pub async fn note_for(&self, slug: &str, event: Event) {
        if let Err(err) = self.note_at_for(slug, event, unix_now()).await {
            tracing::warn!("usage: {}", err.message);
        }
    }

    async fn note_at_for(&self, slug: &str, event: Event, unix_secs: u64) -> ApiResult<()> {
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        record_for(&mut doc, &day_key(unix_secs), slug, event.key(), KEEP_DAYS);
        crate::storage::write_json(&self.path, &doc).await?;
        Ok(())
    }

    pub async fn snapshot(&self) -> ApiResult<Value> {
        let _g = self.lock.lock().await;
        let doc = read_doc(&self.path).await?;
        Ok(with_totals(doc))
    }

    /// The whole counts file.
    pub async fn doc(&self) -> ApiResult<Value> {
        let _g = self.lock.lock().await;
        read_doc(&self.path).await
    }

    /// A thread's finished turns. They are read again only when the thread's stamp moves.
    pub async fn turns_for(
        &self,
        s: &AppState,
        thread: &str,
        stamp: &str,
    ) -> anyhow::Result<Vec<Turn>> {
        if !stamp.is_empty() {
            if let Some((seen, turns)) = self.turns.lock().await.get(thread) {
                if seen == stamp {
                    return Ok(turns.clone());
                }
            }
        }
        let turns = s
            .harness
            .transcript()
            .map_err(|err| anyhow::anyhow!(err.to_string()))?
            .read(thread)
            .await
            .map_err(|err| anyhow::anyhow!(err.to_string()))?
            .turns();
        if !stamp.is_empty() {
            self.turns
                .lock()
                .await
                .insert(thread.to_string(), (stamp.to_string(), turns.clone()));
        }
        Ok(turns)
    }

    /// Messages and attachments this Bot sent through the gateway.
    pub async fn bot_activity(&self, slug: &str) -> ApiResult<(u64, u64)> {
        let _g = self.lock.lock().await;
        let doc = read_doc(&self.path).await?;
        Ok(bot_counts(&doc, slug))
    }
}

async fn read_doc(path: &std::path::Path) -> ApiResult<Value> {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => {
            #[allow(
                clippy::map_err_ignore,
                reason = "serde quotes usage.json; the client only hears that the file is not JSON"
            )]
            let doc: Value = serde_json::from_str(&text)
                .map_err(|_| ApiError::new(500, "usage.json is not valid JSON"))?;
            if !doc.is_object() {
                return Err(ApiError::new(500, "usage.json is not valid JSON"));
            }
            Ok(doc)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Ok(json!({ "version": 1, "days": {} }))
        }
        Err(err) => Err(err.into()),
    }
}

/// Add one count on `day` for the machine, and under `bots[slug]` when `slug` is set.
pub(crate) fn record_for(doc: &mut Value, day: &str, slug: &str, key: &str, keep: usize) {
    record(doc, day, key, keep);
    if slug.is_empty() {
        return;
    }
    let Some(entry) = doc
        .get_mut("days")
        .and_then(|days| days.get_mut(day))
        .filter(|entry| entry.is_object())
    else {
        return;
    };
    if !entry["bots"].is_object() {
        entry["bots"] = json!({});
    }
    let bucket = entry["bots"]
        .as_object_mut()
        .expect("just set")
        .entry(slug.to_string())
        .or_insert_with(|| json!({}));
    if !bucket.is_object() {
        *bucket = json!({});
    }
    let n = bucket[key].as_u64().unwrap_or(0);
    bucket[key] = json!(n + 1);
}

/// Add one count on `day` and drop the oldest days past `keep`.
pub(crate) fn record(doc: &mut Value, day: &str, key: &str, keep: usize) {
    if !doc.is_object() {
        *doc = json!({ "version": 1, "days": {} });
    }
    doc["version"] = json!(1);
    if !doc["days"].is_object() {
        doc["days"] = json!({});
    }
    let days = doc["days"].as_object_mut().expect("just set");
    let entry = days.entry(day.to_string()).or_insert_with(|| json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    let n = entry[key].as_u64().unwrap_or(0);
    entry[key] = json!(n + 1);
    if days.len() > keep {
        let mut keys: Vec<String> = days.keys().cloned().collect();
        keys.sort();
        let drop_n = days.len() - keep;
        for old in keys.into_iter().take(drop_n) {
            days.remove(&old);
        }
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
