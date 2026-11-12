//! The Projects board, reading and writing `bots/.botapp/projects.json`
//! (`{version, seq, items: {id: Item}, activity: []}`):
//! per-kind statuses, field cleaners, link cleaning and de-duplication, `ref:` batches,
//! archive/restore/complete activity, `order`, children moving up on delete, and the
//! query views the Bots use (`overview`, `find`, `detail`). A save is all or nothing, and
//! every change is stamped with who made it and broadcast to open pages.
//!
//! Items stay as JSON objects, so fields this module does not know about survive a re-save.

use crate::{
    platform::error::{ApiError, ApiResult},
    storage::write_atomic,
    vault::now_iso,
};
use serde_json::{json, Map, Value};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    path::PathBuf,
};
use tokio::sync::{broadcast, Mutex};

mod apply;
mod links;
#[cfg(test)]
mod tests;
mod validate;
mod view;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use apply::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use links::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use validate::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use view::*;

#[derive(Debug)]
pub struct Projects {
    file: PathBuf,
    state: Mutex<Option<Value>>,
    pub events: broadcast::Sender<Value>,
}

impl Projects {
    pub fn new(file: PathBuf) -> Self {
        Self {
            file,
            state: Mutex::new(None),
            events: broadcast::channel(64).0,
        }
    }

    /// Reads the file the first time. A missing file is an empty board; an unreadable one is an error,
    /// so a later save can't overwrite it.
    async fn load<'a>(&self, g: &'a mut Option<Value>) -> ApiResult<&'a mut Value> {
        if g.is_none() {
            let raw = match tokio::fs::read_to_string(&self.file).await {
                #[allow(
                    clippy::map_err_ignore,
                    reason = "serde quotes the board file; the client only hears that it is unreadable"
                )]
                Ok(text) => serde_json::from_str::<Value>(&text).map_err(|_| unreadable())?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
                Err(_) => return Err(unreadable()),
            };
            *g = Some(json!({
                "version": raw["version"].as_u64().unwrap_or(0),
                "seq": raw["seq"].as_u64().unwrap_or(0),
                "items": if raw["items"].is_object() { raw["items"].clone() } else { json!({}) },
                "activity": if raw["activity"].is_array() { raw["activity"].clone() } else { json!([]) },
            }));
        }
        g.as_mut().ok_or_else(unreadable)
    }

    async fn persist(&self, next: &Value) -> ApiResult<()> {
        write_atomic(&self.file, next.to_string().as_bytes(), Some(0o600)).await?;
        Ok(())
    }

    /// The whole board: version, every item and the last 60 activity entries.
    pub async fn try_board(&self) -> ApiResult<Value> {
        let mut g = self.state.lock().await;
        let s = self.load(&mut g).await?;
        let items: Vec<Value> = s["items"]
            .as_object()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default();
        let activity = s["activity"].as_array().cloned().unwrap_or_default();
        let tail = activity[activity.len().saturating_sub(BOARD_TAIL)..].to_vec();
        Ok(json!({ "version": s["version"], "items": items, "activity": tail }))
    }

    /// Every item, for the query views.
    pub async fn items(&self) -> ApiResult<Vec<Value>> {
        let mut g = self.state.lock().await;
        let s = self.load(&mut g).await?;
        Ok(s["items"]
            .as_object()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default())
    }

    /// The whole activity log, oldest first.
    pub async fn activity(&self) -> ApiResult<Vec<Value>> {
        let mut g = self.state.lock().await;
        let s = self.load(&mut g).await?;
        Ok(s["activity"].as_array().cloned().unwrap_or_default())
    }

    pub async fn version(&self) -> u64 {
        let mut g = self.state.lock().await;
        self.load(&mut g)
            .await
            .map(|s| s["version"].as_u64().unwrap_or(0))
            .unwrap_or(0)
    }

    /// No id creates; an id changes only what is sent; `null` clears a field.
    pub async fn save(&self, inputs: &[Value], who: Value) -> ApiResult<Value> {
        let mut g = self.state.lock().await;
        let state = self.load(&mut g).await?;
        let mut next = state.clone();
        let stamp = now_iso();
        let Some((saved, entries)) = apply_save(&mut next, inputs, &who, &stamp)? else {
            return Ok(json!({ "version": state["version"], "items": [], "unchanged": true }));
        };
        commit_entries(&mut next, &entries);
        self.persist(&next).await?;
        *state = next;
        let _ = self.events.send(json!({
            "version": state["version"], "items": saved, "removed": [], "activity": entries,
        }));
        Ok(json!({ "version": state["version"], "items": saved }))
    }

    /// Deletes items for good; their children move up a level.
    pub async fn delete(&self, ids: &[String], who: Value) -> ApiResult<Value> {
        let mut g = self.state.lock().await;
        let state = self.load(&mut g).await?;
        let mut next = state.clone();
        let (gone, moved, entries) = apply_remove(&mut next, ids, &who, &now_iso())?;
        commit_entries(&mut next, &entries);
        self.persist(&next).await?;
        *state = next;
        let _ = self.events.send(json!({
            "version": state["version"], "items": moved, "removed": gone, "activity": entries,
        }));
        Ok(json!({ "version": state["version"], "removed": gone }))
    }
}

pub(crate) mod projects_tools; // PROJECTS-HOOK (T3): the Projects MCP tools and the same-origin check.
