//! Reactions, the voice-call log, and roster previews kept beside the harness conversation.
//! T3 Code is the harness wired today; its wire projection doesn't carry these. Other backends
//! may be added. Stored as `thread-meta.json` in the app directory.

use crate::{
    platform::error::{ApiError, ApiResult},
    storage,
};
use futures_util::future::join_all;
use regex::Regex;
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    future::Future,
    path::{Path, PathBuf},
    sync::LazyLock,
};
use tokio::sync::Mutex;

mod preview;
mod state;
#[cfg(test)]
mod tests;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use preview::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use state::*;

#[derive(Debug)]
pub struct ThreadMeta {
    file: PathBuf,
    state: Mutex<Option<Map<String, Value>>>,
    /// Per thread: the stamp its preview was computed at, and the preview (None when it had none).
    previews: Mutex<HashMap<String, (String, Option<Value>)>>,
}

impl ThreadMeta {
    pub fn new(file: PathBuf) -> Self {
        Self {
            file,
            state: Mutex::new(None),
            previews: Mutex::new(HashMap::new()),
        }
    }

    pub async fn get(&self, thread: Option<&str>) -> ApiResult<Value> {
        let thread = checked(thread, &THREAD_ID, "thread")?;
        let mut slot = self.state.lock().await;
        let state = loaded(&self.file, &mut slot).await?;
        Ok(view(state, thread))
    }

    /// Adds the reaction if it is absent, removes it if present. Returns the thread's view.
    pub async fn toggle_reaction(
        &self,
        thread: Option<&str>,
        item: Option<&str>,
        emoji: Option<&str>,
    ) -> ApiResult<Value> {
        let thread = checked(thread, &THREAD_ID, "thread")?;
        let item = checked(item, &ITEM_ID, "message")?;
        let emoji = emoji
            .filter(|e| valid_emoji(e))
            .ok_or_else(|| bad("invalid reaction"))?;
        let mut slot = self.state.lock().await;
        let state = loaded(&self.file, &mut slot).await?;
        let entry = thread_entry(state, thread)?;
        let reactions = entry
            .entry("reactions")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(corrupt)?;
        let current: Vec<String> = reactions
            .get(item)
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let next: Vec<String> = if current.iter().any(|e| e == emoji) {
            current.into_iter().filter(|e| e != emoji).collect()
        } else {
            current.into_iter().chain([emoji.to_string()]).collect()
        };
        if next.is_empty() {
            reactions.remove(item);
        } else {
            reactions.insert(item.to_string(), json!(next));
        }
        save(&self.file, state).await?;
        Ok(view(state, thread))
    }

    /// Records a call of `seconds` (0 to 86400), rounded, at the time `at` (an ISO stamp).
    pub async fn add_voice_call(
        &self,
        thread: Option<&str>,
        seconds: Option<&Value>,
        at: &str,
    ) -> ApiResult<Value> {
        let thread = checked(thread, &THREAD_ID, "thread")?;
        let seconds = seconds
            .and_then(Value::as_f64)
            .filter(|s| s.is_finite() && (0.0..=86_400.0).contains(s))
            .ok_or_else(|| bad("invalid duration"))?;
        let mut slot = self.state.lock().await;
        let state = loaded(&self.file, &mut slot).await?;
        let entry = thread_entry(state, thread)?;
        entry
            .entry("voiceCalls")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(corrupt)?
            .push(json!({ "at": at, "seconds": rounded_seconds(seconds) }));
        save(&self.file, state).await?;
        Ok(view(state, thread))
    }

    /// Adds the message id if it is absent, removes it if present. Past [`PIN_CAP`] is refused.
    pub async fn toggle_pin(&self, thread: Option<&str>, item: Option<&str>) -> ApiResult<Value> {
        let thread = checked(thread, &THREAD_ID, "thread")?;
        let item = checked(item, &ITEM_ID, "message")?;
        let mut slot = self.state.lock().await;
        let state = loaded(&self.file, &mut slot).await?;
        let entry = thread_entry(state, thread)?;
        let mut pins: Vec<String> = entry
            .get("pins")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(index) = pins.iter().position(|id| id == item) {
            pins.remove(index);
        } else if pins.len() >= PIN_CAP {
            return Err(bad("That conversation already has 20 pins."));
        } else {
            pins.push(item.to_string());
        }
        entry.insert("pins".into(), json!(pins));
        save(&self.file, state).await?;
        Ok(view(state, thread))
    }

    /// Stores `replyTo` under the message id the gateway already assigned.
    pub async fn set_reply(
        &self,
        thread: Option<&str>,
        message_id: Option<&str>,
        reply_to: Option<&str>,
        quote: Option<&str>,
    ) -> ApiResult<Value> {
        let thread = checked(thread, &THREAD_ID, "thread")?;
        let message_id = checked(message_id, &ITEM_ID, "message")?;
        let reply_to = checked(reply_to, &ITEM_ID, "message")?;
        let quote: String = quote.unwrap_or("").chars().take(500).collect();
        let mut slot = self.state.lock().await;
        let state = loaded(&self.file, &mut slot).await?;
        let entry = thread_entry(state, thread)?;
        let replies = entry
            .entry("replyTo")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(corrupt)?;
        replies.insert(
            message_id.to_string(),
            json!({ "id": reply_to, "text": quote }),
        );
        save(&self.file, state).await?;
        Ok(view(state, thread))
    }

    /// Remembers the newest message id, and advances last-read only while the chat is open.
    pub async fn mark_read(
        &self,
        thread: Option<&str>,
        latest: Option<&str>,
        open: bool,
        hidden: bool,
    ) -> ApiResult<Value> {
        let thread = checked(thread, &THREAD_ID, "thread")?;
        let latest = match latest {
            Some(id) if !id.is_empty() => checked(Some(id), &ITEM_ID, "message")?,
            _ => "",
        };
        let mut slot = self.state.lock().await;
        let state = loaded(&self.file, &mut slot).await?;
        let entry = thread_entry(state, thread)?;
        let stored = entry
            .get("lastRead")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let known = if latest.is_empty() {
            entry
                .get("latest")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        } else {
            latest.to_string()
        };
        if !known.is_empty() {
            entry.insert("latest".into(), json!(known));
        }
        let next = next_last_read(&stored, &known, open, hidden);
        entry.insert("lastRead".into(), json!(next));
        save(&self.file, state).await?;
        let mut seen = view(state, thread);
        seen["unread"] = json!(is_unread(&stored, &known, open, hidden));
        Ok(seen)
    }

    /// The threads whose preview must be computed again: new, or with a moved stamp.
    pub async fn stale(&self, stamps: Option<&Value>) -> ApiResult<Vec<(String, String)>> {
        let entries = stamp_entries(stamps)?;
        let cache = self.previews.lock().await;
        Ok(entries
            .into_iter()
            .filter(|(id, stamp)| cache.get(id).is_none_or(|(s, _)| s != stamp))
            .collect())
    }

    /// Keeps a computed preview. A failed load is never recorded, so the earlier preview stays.
    pub async fn remember(&self, thread: String, stamp: String, preview: Option<Value>) {
        self.previews.lock().await.insert(thread, (stamp, preview));
    }

    /// The previews for `stamps` whose stamp is current and that have one.
    pub async fn current(&self, stamps: Option<&Value>) -> ApiResult<Value> {
        let entries = stamp_entries(stamps)?;
        let cache = self.previews.lock().await;
        let mut out = Map::new();
        for (id, stamp) in entries {
            if let Some((s, Some(preview))) = cache.get(&id) {
                if *s == stamp {
                    out.insert(id, preview.clone());
                }
            }
        }
        Ok(Value::Object(out))
    }

    /// Recomputes the stale previews with `fetch` (at most four loads at a time), then returns them all.
    /// A thread whose projection fails keeps its earlier preview and is retried on the next call.
    pub async fn previews<F, Fut>(
        &self,
        stamps: Option<&Value>,
        _names: &HashMap<String, String>,
        fetch: F,
    ) -> ApiResult<Value>
    where
        F: Fn(String) -> Fut,
        Fut: Future<Output = anyhow::Result<Option<Value>>>,
    {
        for chunk in self.stale(stamps).await?.chunks(4) {
            let loads = chunk.iter().map(|(id, stamp)| {
                let load = fetch(id.clone());
                async move { (id, stamp, load.await) }
            });
            for (id, stamp, result) in join_all(loads).await {
                if let Ok(preview) = result {
                    self.remember(id.clone(), stamp.clone(), preview).await;
                }
            }
        }
        self.current(stamps).await
    }
}
