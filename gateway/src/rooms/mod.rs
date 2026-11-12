//! A local room of Bots. This is not a T3 channel and not the Working on list.
//! The file is `groups.json` in the app directory: an id, a title, a description, optional
//! colour and shape (kept for older rooms; the app shows stacked member faces), member slugs,
//! and the messages stored on this machine. `lastSeenAt` is when the person last opened the
//! room. `notifications` is the mute switch: missing means on. Mute does not clear unread.
//!
//! A send is stored here, then dispatched to the members it wakes: everyone named with `@`,
//! `@everyone` or `@all` for the whole group, a single plain name, otherwise the room's answer
//! policy. `@team` and `@group` are not whole-group wake words. A member whose slug or name is
//! `everyone` or `all` is that member. `last` (the
//! default, and what a room without the field does) wakes whoever spoke last, or everyone while
//! no Bot has spoken. `everyone` wakes every member. `chief` wakes only the chief, and that
//! dispatch carries the chief line. Each dispatch remembers the message id it created, its
//! anchor. A sync reads each member's group thread and copies the replies after that anchor into
//! the room. A reply that @-mentions another member can wake that member within the hop budget
//! ([`max_hops`]: one for `last` and `everyone`, one less than the member count for `chief`). A
//! courtesy reply never wakes anyone. `answerPolicy` and `chief` are optional. Missing fields are
//! not filled in on disk.
//!
//! `working` and `workingSlugs` on a list row are computed from each member's group thread.
//! They are not written to `groups.json`.

use crate::error::{ApiError, ApiResult};
use futures_util::future::join_all;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

mod dispatch;
mod edit;
mod members;
mod policy;
#[cfg(test)]
mod tests;
mod working;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use dispatch::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use edit::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use members::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use policy::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use working::*;

#[derive(Debug)]
pub struct Rooms {
    path: PathBuf,
    lock: Mutex<()>,
}

impl Rooms {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    /// Rooms on disk. A room that predates `lastSeenAt` is marked seen now, once, so an old
    /// transcript does not start out unread.
    pub async fn list(&self) -> ApiResult<Vec<Value>> {
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        let mut stamped = false;
        let now = crate::vault::now_iso();
        if let Some(rooms) = doc.get_mut("rooms").and_then(Value::as_array_mut) {
            for room in rooms {
                if room["lastSeenAt"].as_str().is_none() {
                    room["lastSeenAt"] = json!(now);
                    stamped = true;
                }
            }
        }
        if stamped {
            write_doc(&self.path, &doc).await?;
        }
        Ok(rooms_of(&doc))
    }

    pub async fn get(&self, room_id: &str) -> ApiResult<Value> {
        let _g = self.lock.lock().await;
        let doc = read_doc(&self.path).await?;
        rooms_of(&doc)
            .into_iter()
            .find(|room| room["id"].as_str() == Some(room_id))
            .ok_or_else(|| ApiError::not_found("No such group"))
    }
}

fn message(id: &str, at: &str, from: &str, name: &str, text: &str) -> Value {
    json!({ "id": id, "at": at, "from": from, "name": name, "text": text })
}

fn has_message(room: &Value, id: &str) -> bool {
    room["messages"].as_array().is_some_and(|list| {
        list.iter()
            .any(|message| message["id"].as_str() == Some(id))
    })
}

fn push_message(room: &mut Value, message: &Value) -> ApiResult<()> {
    let list = room
        .as_object_mut()
        .and_then(|room| room.get_mut("messages"))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| ApiError::new(500, "groups.json is not valid"))?;
    list.push(message.clone());
    Ok(())
}

fn rooms_of(doc: &Value) -> Vec<Value> {
    doc["rooms"].as_array().cloned().unwrap_or_default()
}

fn push_room(doc: &mut Value, room: Value) -> ApiResult<()> {
    let list = doc
        .as_object_mut()
        .and_then(|doc| doc.get_mut("rooms"))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| ApiError::new(500, "groups.json is not valid"))?;
    list.push(room);
    Ok(())
}

fn room_mut<'a>(doc: &'a mut Value, room_id: &str) -> ApiResult<&'a mut Value> {
    doc.get_mut("rooms")
        .and_then(Value::as_array_mut)
        .and_then(|rooms| {
            rooms
                .iter_mut()
                .find(|room| room["id"].as_str() == Some(room_id))
        })
        .ok_or_else(|| ApiError::not_found("No such room"))
}

async fn read_doc(path: &std::path::Path) -> ApiResult<Value> {
    match tokio::fs::read_to_string(path).await {
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
            Ok(doc)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Ok(json!({ "version": 1, "rooms": [] }))
        }
        Err(err) => Err(err.into()),
    }
}

async fn write_doc(path: &std::path::Path, doc: &Value) -> ApiResult<()> {
    crate::storage::write_json(path, doc).await?;
    Ok(())
}
