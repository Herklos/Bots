//! One Bot's library: files under `library/`, conversation links, attachment records,
//! and the records the person keeps (projects, notes, tasks, plans, and the other
//! board kinds). Every read and write goes through [`crate::storage::workspace::Workspace`].

use crate::bots::usage::day_key;
use crate::platform::error::{ApiError, ApiResult};
use crate::storage::workspace::{DirEntry, Workspace};
use base64::Engine;
use serde_json::{json, Value};
use std::sync::Arc;

mod agents_hook;
mod connectors;
mod kinds;
#[cfg(test)]
mod tests;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use agents_hook::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use connectors::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use kinds::*;

#[derive(Debug)]
pub struct Library {
    ws: Arc<dyn Workspace>,
    records: Arc<dyn crate::storage::stores::RecordStore>,
    connectors: Arc<dyn crate::storage::stores::ConnectorStore>,
    mcp_url: String,
    mcp_token: String,
}

impl Library {
    pub fn new(ws: Arc<dyn Workspace>, mcp_url: String, mcp_token: String) -> Self {
        let records = Arc::new(
            crate::storage::adapters::filesystem::FilesystemRecordStore::new(Arc::clone(&ws)),
        );
        Self::with_record_store(ws, mcp_url, mcp_token, records)
    }

    pub(crate) fn with_record_store(
        ws: Arc<dyn Workspace>,
        mcp_url: String,
        mcp_token: String,
        records: Arc<dyn crate::storage::stores::RecordStore>,
    ) -> Self {
        let connectors = Arc::new(
            crate::storage::adapters::filesystem::FilesystemConnectorStore::new(Arc::clone(&ws)),
        );
        Self {
            ws,
            records,
            connectors,
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

    pub(crate) fn with_connector_store(
        mut self,
        connectors: Arc<dyn crate::storage::stores::ConnectorStore>,
    ) -> Self {
        self.connectors = connectors;
        self
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

    async fn records(&self, slug: &str, today: &str, week: &str) -> ApiResult<Vec<Value>> {
        let mut out = Vec::new();
        for item in self.records.list(slug).await? {
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
        let id = input["id"].as_str().filter(|s| !s.is_empty());
        let create = id.is_none();
        let id = id.map(str::to_string).unwrap_or_else(new_id);
        let item = json!({ "id": id, "kind": kind, "title": title, "status": status,
            "body": body, "at": crate::vault::now_iso() });
        let _ = now;
        self.records.put(slug, &item, create).await?;
        Ok(json!({ "ok": true }))
    }

    pub async fn delete_item(&self, slug: &str, id: &str) -> ApiResult<Value> {
        Self::require_slug(slug)?;
        self.records.delete(slug, id).await?;
        Ok(json!({ "ok": true }))
    }

    /// Bytes of one file under `library/`. The path is workspace-relative (`library/name`).
    /// A symlink is refused. An image extension must match its signature and stay within 5 MiB.
    pub(crate) async fn file_bytes(&self, slug: &str, path: &str) -> Result<Vec<u8>, FileError> {
        Self::require_slug(slug).map_err(|_err| FileError::BadPath)?;
        let rel = crate::chat::images::library_rel(path).ok_or(FileError::BadPath)?;
        let bytes =
            crate::chat::images::read_regular(self.ws.as_ref(), &format!("{slug}/{rel}")).await?;
        if let Some(kind) = crate::chat::images::kind_of(&rel) {
            if bytes.len() > crate::chat::images::MAX_IMAGE_BYTES {
                return Err(FileError::TooBig);
            }
            if crate::chat::images::picture(kind, &bytes).is_none() {
                return Err(FileError::NotImage);
            }
        }
        Ok(bytes)
    }
}

/// Why [`Library::file_bytes`] refused a path. The route maps each one to a status.
#[derive(Debug)]
pub(crate) enum FileError {
    BadPath,
    Missing,
    Link,
    TooBig,
    NotImage,
    Io,
}

impl From<crate::chat::images::ImageFault> for FileError {
    fn from(fault: crate::chat::images::ImageFault) -> Self {
        match fault {
            crate::chat::images::ImageFault::Missing => Self::Missing,
            crate::chat::images::ImageFault::Link => Self::Link,
            crate::chat::images::ImageFault::BadPath => Self::BadPath,
            crate::chat::images::ImageFault::Io => Self::Io,
            crate::chat::images::ImageFault::TooBig => Self::TooBig,
            crate::chat::images::ImageFault::NotImage => Self::NotImage,
        }
    }
}

impl FileError {
    pub(crate) fn api(self) -> ApiError {
        match self {
            Self::BadPath | Self::Link => ApiError::bad_request("That file is not in the library."),
            Self::Missing => ApiError::not_found("no such file"),
            Self::TooBig => ApiError::new(413, "That image is larger than 5 MiB."),
            Self::NotImage => ApiError::new(415, "That file is not an image."),
            Self::Io => {
                tracing::error!("library file could not be read");
                ApiError::new(500, "The file could not be read.")
            }
        }
    }
}
