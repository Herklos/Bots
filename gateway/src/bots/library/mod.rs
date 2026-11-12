//! One Bot's library: files under `library/`, conversation links, attachment records,
//! and the records the person keeps (projects, notes, tasks, plans, and the other
//! board kinds). Every read and write goes through [`crate::storage::workspace::Workspace`].

use crate::bots::usage::day_key;
use crate::platform::error::{ApiError, ApiResult};
use crate::storage::workspace::{DirEntry, Workspace};
use base64::Engine;
use serde_json::{json, Value};
use std::sync::Arc;

mod connectors;
mod kinds;
#[cfg(test)]
mod tests;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use connectors::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use kinds::*;

#[derive(Debug)]
pub struct Library {
    ws: Arc<dyn Workspace>,
    mcp_url: String,
    mcp_token: String,
}

impl Library {
    pub fn new(ws: Arc<dyn Workspace>, mcp_url: String, mcp_token: String) -> Self {
        Self {
            ws,
            mcp_url,
            mcp_token,
        }
    }

    pub fn root_for(&self, slug: &str) -> String {
        self.ws.root_for_harness(slug)
    }

    fn require_slug(slug: &str) -> ApiResult<()> {
        if crate::bots::valid_slug(slug) {
            Ok(())
        } else {
            Err(ApiError::bad_request("bad slug"))
        }
    }

    fn records_path(slug: &str) -> String {
        format!("{slug}/.bot/library.json")
    }

    fn connectors_path(slug: &str) -> String {
        format!("{slug}/.bot/connectors.json")
    }

    pub async fn view(&self, slug: &str, messages: &[Value], now: u64) -> ApiResult<Value> {
        Self::require_slug(slug)?;
        let today = day_key(now);
        let week = day_key(now.saturating_sub(6 * 86_400));
        let mut entries = Vec::new();
        for item in self.files(slug, &today, &week).await? {
            entries.push(item);
        }
        for item in self.attachments(slug, &today, &week).await? {
            entries.push(item);
        }
        for item in self.records(slug, &today, &week).await? {
            entries.push(item);
        }
        for href in messages
            .iter()
            .flat_map(|m| web_links(m["text"].as_str().unwrap_or("")))
        {
            let day = messages
                .iter()
                .find(|m| m["text"].as_str().unwrap_or("").contains(&href))
                .and_then(|m| m["at"].as_str())
                .unwrap_or("");
            let day = if day.len() >= 10 { &day[..10] } else { "" };
            entries.push(json!({
                "id": href,
                "kind": "link",
                "title": detail_host(&href),
                "detail": href,
                "group": group_day(day, &today, &week),
                "status": "",
                "body": "",
            }));
        }
        entries.sort_by(|a, b| {
            b["group"]
                .as_str()
                .cmp(&a["group"].as_str())
                .then(a["title"].as_str().cmp(&b["title"].as_str()))
        });
        Ok(json!({
            "ok": true,
            "entries": entries,
            "connectors": self.public_connectors(slug).await?,
        }))
    }

    async fn files(&self, slug: &str, today: &str, week: &str) -> ApiResult<Vec<Value>> {
        let listed = self.ws.list(&format!("{slug}/library")).await?;
        Ok(listed
            .into_iter()
            .filter(|e| e.is_file)
            .map(|e| file_entry(slug, e, today, week))
            .collect())
    }

    async fn attachments(&self, slug: &str, today: &str, week: &str) -> ApiResult<Vec<Value>> {
        let listed = self.ws.list(&format!("{slug}/.bot/attachments")).await?;
        let mut out = Vec::new();
        for entry in listed {
            if !entry.is_file || !ascii_ext(&entry.name, "json") {
                continue;
            }
            let rel = format!("{slug}/.bot/attachments/{}", entry.name);
            let Ok(bytes) = self.ws.read(&rel).await else {
                continue;
            };
            let Ok(meta) = serde_json::from_slice::<Value>(&bytes) else {
                continue;
            };
            let id = meta["id"].as_str().unwrap_or("").to_string();
            if id.is_empty() {
                continue;
            }
            let name = meta["name"].as_str().unwrap_or("Attachment");
            let mime = meta["mediaType"].as_str().unwrap_or("");
            let at = meta["createdAt"].as_str().unwrap_or("");
            let day = if at.len() >= 10 { &at[..10] } else { "" };
            out.push(json!({
                "id": format!("attachment:{id}"),
                "kind": media_kind(mime),
                "title": name,
                "detail": if mime.is_empty() { "File" } else { mime },
                "group": group_day(day, today, week),
                "status": "",
                "body": "",
            }));
        }
        Ok(out)
    }

    async fn read_records(&self, slug: &str) -> ApiResult<Vec<Value>> {
        match self.ws.read(&Self::records_path(slug)).await {
            Ok(bytes) => {
                #[allow(
                    clippy::map_err_ignore,
                    reason = "serde quotes library.json, which holds the user's records"
                )]
                let doc: Value = serde_json::from_slice(&bytes)
                    .map_err(|_| ApiError::new(500, "library.json is not valid JSON"))?;
                Ok(doc["items"].as_array().cloned().unwrap_or_default())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    async fn write_records(&self, slug: &str, items: &[Value]) -> ApiResult<()> {
        let text = serde_json::to_string_pretty(&json!({ "version": 1, "items": items }))
            .unwrap_or_else(|_| "{}".into())
            + "\n";
        self.ws
            .write(&Self::records_path(slug), text.as_bytes(), Some(0o600))
            .await?;
        Ok(())
    }

    async fn records(&self, slug: &str, today: &str, week: &str) -> ApiResult<Vec<Value>> {
        let mut out = Vec::new();
        for item in self.read_records(slug).await? {
            let at = item["at"].as_str().unwrap_or("");
            let day = if at.len() >= 10 { &at[..10] } else { "" };
            out.push(json!({
                "id": item["id"],
                "kind": item["kind"],
                "title": item["title"],
                "detail": item["status"].as_str().unwrap_or(""),
                "group": group_day(day, today, week),
                "status": item["status"].as_str().unwrap_or(""),
                "body": item["body"].as_str().unwrap_or(""),
            }));
        }
        Ok(out)
    }

    pub async fn save_item(&self, slug: &str, input: &Value, now: u64) -> ApiResult<Value> {
        Self::require_slug(slug)?;
        let kind = input["kind"].as_str().unwrap_or("").trim();
        if !RECORD_KINDS.contains(&kind) {
            return Err(ApiError::bad_request(format!(
                "kind must be one of {}.",
                RECORD_KINDS.join(", ")
            )));
        }
        let title = input["title"].as_str().unwrap_or("").trim();
        if title.is_empty() {
            return Err(ApiError::bad_request("Give it a title."));
        }
        let allowed = statuses(kind);
        let mut status = input["status"].as_str().unwrap_or("").trim().to_string();
        if status.is_empty() {
            status = default_status(kind).into();
        }
        if !allowed.is_empty() && !allowed.contains(&status.as_str()) {
            return Err(ApiError::bad_request(format!(
                "A {kind}'s status is one of {}.",
                allowed.join(", ")
            )));
        }
        if allowed.is_empty() {
            status.clear();
        }
        let body = input["body"].as_str().unwrap_or("").to_string();
        let mut items = self.read_records(slug).await?;
        let id = input["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let at = crate::vault::now_iso();
        let _ = now;
        if let Some(id) = id {
            let Some(found) = items.iter_mut().find(|i| i["id"] == id) else {
                return Err(ApiError::not_found("no such library item"));
            };
            found["kind"] = json!(kind);
            found["title"] = json!(title);
            found["status"] = json!(status);
            found["body"] = json!(body);
        } else {
            items.push(json!({
                "id": new_id(),
                "kind": kind,
                "title": title,
                "status": status,
                "body": body,
                "at": at,
            }));
        }
        self.write_records(slug, &items).await?;
        Ok(json!({ "ok": true }))
    }

    pub async fn delete_item(&self, slug: &str, id: &str) -> ApiResult<Value> {
        Self::require_slug(slug)?;
        let mut items = self.read_records(slug).await?;
        let before = items.len();
        items.retain(|i| i["id"] != id);
        if items.len() == before {
            return Err(ApiError::not_found("no such library item"));
        }
        self.write_records(slug, &items).await?;
        Ok(json!({ "ok": true }))
    }

    /// Bytes of one file under `library/`. The path is workspace-relative (`library/name`).
    pub async fn file_bytes(&self, slug: &str, path: &str) -> ApiResult<Vec<u8>> {
        Self::require_slug(slug)?;
        let rel = path.trim_start_matches('/');
        if !rel.starts_with("library/") || rel.contains("..") {
            return Err(ApiError::bad_request("That file is not in the library."));
        }
        let full = format!("{slug}/{rel}");
        self.ws.read(&full).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ApiError::not_found("no such file")
            } else if e.kind() == std::io::ErrorKind::InvalidInput {
                ApiError::bad_request("That file is not in the library.")
            } else {
                e.into()
            }
        })
    }
}
