//! The Projects board, reading and writing hbots `bots/.botapp/projects.json`
//! (`{version, seq, items: {id: Item}, activity: []}`). A save is all or nothing, every change
//! is stamped with who made it and broadcast to open pages (Server-Sent Events).
//! A deliberately smaller port of `server/projects.mts`: fields are kept as sent (bounded),
//! with the kinds and the links' shape checked.
use crate::{
    error::{ApiError, ApiResult},
    store::write_atomic,
    vault::now_iso,
};
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use tokio::sync::{broadcast, Mutex};

const KINDS: &[&str] = &[
    "project", "task", "machine", "service", "repo", "note", "link",
];
const MAX_BATCH: usize = 100;
const MAX_ACTIVITY: usize = 300;
const TEXT_FIELDS: &[&str] = &[
    "title",
    "status",
    "area",
    "body",
    "parentId",
    "machineId",
    "doneAt",
];

pub struct Projects {
    file: PathBuf,
    state: Mutex<Option<Value>>,
    pub events: broadcast::Sender<Value>,
}

fn new_id() -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    (0..8)
        .map(|_| ALPHABET[rand::random::<usize>() % ALPHABET.len()] as char)
        .collect()
}

/// `{type: "bot", name, slug}` for a Bot, otherwise the user.
pub fn actor(input: Option<&Value>) -> Value {
    match input {
        Some(a) if a["type"] == "bot" => {
            json!({ "type": "bot", "name": a["name"], "slug": a["slug"] })
        }
        _ => json!({ "type": "user" }),
    }
}

impl Projects {
    pub fn new(file: PathBuf) -> Self {
        Self {
            file,
            state: Mutex::new(None),
            events: broadcast::channel(64).0,
        }
    }

    async fn load<'a>(&self, g: &'a mut Option<Value>) -> &'a mut Value {
        if g.is_none() {
            let raw = crate::store::read_json(&self.file)
                .await
                .unwrap_or(Value::Null);
            *g = Some(json!({
                "version": raw["version"].as_u64().unwrap_or(0),
                "seq": raw["seq"].as_u64().unwrap_or(0),
                "items": if raw["items"].is_object() { raw["items"].clone() } else { json!({}) },
                "activity": if raw["activity"].is_array() { raw["activity"].clone() } else { json!([]) },
            }));
        }
        g.as_mut().expect("just loaded")
    }

    async fn persist(&self, state: &Value) -> ApiResult<()> {
        write_atomic(&self.file, state.to_string().as_bytes(), Some(0o600)).await?;
        Ok(())
    }

    pub async fn board(&self) -> Value {
        let mut g = self.state.lock().await;
        let s = self.load(&mut g).await;
        let items: Vec<Value> = s["items"]
            .as_object()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default();
        let activity = s["activity"].as_array().cloned().unwrap_or_default();
        let tail = activity[activity.len().saturating_sub(60)..].to_vec();
        json!({ "version": s["version"], "items": items, "activity": tail })
    }

    pub async fn version(&self) -> u64 {
        let mut g = self.state.lock().await;
        self.load(&mut g).await["version"].as_u64().unwrap_or(0)
    }

    /// No id creates; an id changes only what is sent; `null` clears a field.
    pub async fn save(&self, inputs: &[Value], who: Value) -> ApiResult<Value> {
        if inputs.is_empty() || inputs.len() > MAX_BATCH {
            return Err(ApiError::bad_request(format!(
                "Send between 1 and {MAX_BATCH} items."
            )));
        }
        let mut g = self.state.lock().await;
        let state = self.load(&mut g).await;
        let mut next = state.clone();
        let now = now_iso();
        let mut changed = Vec::new();
        let mut entries = Vec::new();
        for input in inputs {
            let input = input
                .as_object()
                .ok_or_else(|| ApiError::bad_request("Each item must be an object."))?;
            let id = input.get("id").and_then(Value::as_str).map(String::from);
            let existing = id.as_ref().and_then(|id| next["items"].get(id).cloned());
            if id.is_some() && existing.is_none() {
                return Err(ApiError::not_found(format!(
                    "No item {}.",
                    id.unwrap_or_default()
                )));
            }
            let created = existing.is_none();
            let mut item: Map<String, Value> = existing
                .as_ref()
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_else(|| {
                    let mut m = Map::new();
                    m.insert("id".into(), json!(new_id()));
                    m.insert("fields".into(), json!({}));
                    m.insert("links".into(), json!([]));
                    m.insert("pinned".into(), json!(false));
                    m.insert("archived".into(), json!(false));
                    m.insert("createdAt".into(), json!(now));
                    m.insert("createdBy".into(), who.clone());
                    m
                });
            if let Some(kind) = input.get("kind") {
                let k = kind.as_str().filter(|k| KINDS.contains(k)).ok_or_else(|| {
                    ApiError::bad_request(format!("kind must be one of {}.", KINDS.join(", ")))
                })?;
                item.insert("kind".into(), json!(k));
            }
            if !item.contains_key("kind") {
                return Err(ApiError::bad_request("A new item needs a kind."));
            }
            let mut keys = Vec::new();
            for f in TEXT_FIELDS {
                match input.get(*f) {
                    None => {}
                    Some(Value::Null) if *f != "title" => {
                        item.remove(*f);
                        keys.push(*f);
                    }
                    Some(Value::String(s)) => {
                        let limit = if *f == "body" { 20_000 } else { 500 };
                        item.insert(
                            (*f).into(),
                            json!(s.chars().take(limit).collect::<String>()),
                        );
                        keys.push(*f);
                    }
                    Some(_) => return Err(ApiError::bad_request(format!("{f} must be text."))),
                }
            }
            if item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .is_empty()
            {
                return Err(ApiError::bad_request("An item needs a title."));
            }
            for f in ["pinned", "archived"] {
                if let Some(v) = input.get(f) {
                    item.insert(f.into(), json!(v == &Value::Bool(true)));
                    keys.push(f);
                }
            }
            if let Some(fields) = input.get("fields") {
                let mut current = item
                    .get("fields")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                for (k, v) in fields
                    .as_object()
                    .ok_or_else(|| ApiError::bad_request("fields must be an object."))?
                {
                    if v.is_null() {
                        current.remove(k);
                    } else {
                        current.insert(k.clone(), v.clone());
                    }
                }
                item.insert("fields".into(), Value::Object(current));
                keys.push("fields");
            }
            if let Some(links) = input.get("links") {
                let list = links
                    .as_array()
                    .filter(|l| l.len() <= 50 && l.iter().all(|l| l["type"].is_string()))
                    .ok_or_else(|| ApiError::bad_request("links must be a list of {type, …}."))?;
                item.insert("links".into(), json!(list));
                keys.push("links");
            }
            if !created && keys.is_empty() {
                continue;
            }
            item.insert("updatedAt".into(), json!(now));
            item.insert("updatedBy".into(), who.clone());
            let item = Value::Object(item);
            let item_id = item["id"].as_str().unwrap_or_default().to_string();
            next["seq"] = json!(next["seq"].as_u64().unwrap_or(0) + 1);
            entries.push(json!({
                "seq": next["seq"], "at": now, "actor": who, "itemId": item_id, "kind": item["kind"], "title": item["title"],
                "action": if created { "created" } else if input.get("archived") == Some(&json!(true)) { "archived" } else { "updated" },
                "changed": keys,
            }));
            next["items"][&item_id] = item.clone();
            changed.push(item);
        }
        if changed.is_empty() {
            return Ok(json!({ "version": state["version"], "items": [], "unchanged": true }));
        }
        next["version"] = json!(next["version"].as_u64().unwrap_or(0) + 1);
        let mut activity = next["activity"].as_array().cloned().unwrap_or_default();
        activity.extend(entries.iter().cloned());
        let excess = activity.len().saturating_sub(MAX_ACTIVITY);
        next["activity"] = json!(activity[excess..]);
        self.persist(&next).await?;
        *state = next;
        let _ = self.events.send(json!({ "version": state["version"], "items": changed, "removed": [], "activity": entries }));
        Ok(json!({ "version": state["version"], "items": changed }))
    }

    /// Deletes items; their children move up a level.
    pub async fn delete(&self, ids: &[String], who: Value) -> ApiResult<Value> {
        let mut g = self.state.lock().await;
        let state = self.load(&mut g).await;
        let mut next = state.clone();
        let now = now_iso();
        let mut gone = Vec::new();
        let mut moved = Vec::new();
        let mut entries = Vec::new();
        for id in ids {
            let Some(item) = next["items"].as_object_mut().and_then(|m| m.remove(id)) else {
                continue;
            };
            next["seq"] = json!(next["seq"].as_u64().unwrap_or(0) + 1);
            entries.push(json!({ "seq": next["seq"], "at": now, "actor": who, "itemId": id, "kind": item["kind"], "title": item["title"], "action": "deleted", "changed": [] }));
            gone.push(id.clone());
        }
        if let Some(items) = next["items"].as_object_mut() {
            for item in items.values_mut() {
                if item["parentId"]
                    .as_str()
                    .is_some_and(|p| gone.iter().any(|g| g == p))
                {
                    item.as_object_mut().map(|m| m.remove("parentId"));
                    item["updatedAt"] = json!(now);
                    moved.push(item.clone());
                }
            }
        }
        if gone.is_empty() {
            return Ok(json!({ "version": state["version"], "removed": [] }));
        }
        next["version"] = json!(next["version"].as_u64().unwrap_or(0) + 1);
        let mut activity = next["activity"].as_array().cloned().unwrap_or_default();
        activity.extend(entries.iter().cloned());
        let excess = activity.len().saturating_sub(MAX_ACTIVITY);
        next["activity"] = json!(activity[excess..]);
        self.persist(&next).await?;
        *state = next;
        let _ = self.events.send(json!({ "version": state["version"], "items": moved, "removed": gone, "activity": entries }));
        Ok(json!({ "version": state["version"], "removed": gone }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn save_update_delete_broadcast() {
        let dir = tempfile::tempdir().unwrap();
        let p = Projects::new(dir.path().join("projects.json"));
        let mut rx = p.events.subscribe();
        let saved = p
            .save(
                &[json!({ "kind": "project", "title": "Ship it" })],
                actor(None),
            )
            .await
            .unwrap();
        let id = saved["items"][0]["id"].as_str().unwrap().to_string();
        assert_eq!(id.len(), 8);
        assert_eq!(rx.recv().await.unwrap()["version"], 1);
        let child = p
            .save(
                &[json!({ "kind": "task", "title": "Write tests", "parentId": id })],
                actor(Some(
                    &json!({ "type": "bot", "name": "Ada", "slug": "ada" }),
                )),
            )
            .await
            .unwrap();
        assert_eq!(child["items"][0]["createdBy"]["type"], "bot");
        assert!(p
            .save(&[json!({ "kind": "nope", "title": "x" })], actor(None))
            .await
            .is_err());
        let unchanged = p.save(&[json!({ "id": id })], actor(None)).await.unwrap();
        assert_eq!(unchanged["unchanged"], true);
        p.delete(std::slice::from_ref(&id), actor(None))
            .await
            .unwrap();
        let reloaded = Projects::new(dir.path().join("projects.json"))
            .board()
            .await;
        assert_eq!(reloaded["version"], 3);
        assert!(
            reloaded["items"][0].get("parentId").is_none(),
            "the child moved up a level"
        );
    }
}
