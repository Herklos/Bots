//! One Bot's library: files under `library/`, conversation links, attachment records,
//! and the records the person keeps (projects, notes, tasks, plans, and the other
//! board kinds). Every read and write goes through [`crate::workspace::Workspace`].
use crate::error::{ApiError, ApiResult};
use crate::usage::day_key;
use crate::workspace::{DirEntry, Workspace};
use base64::Engine;
use serde_json::{json, Value};
use std::sync::Arc;

pub const RECORD_KINDS: &[&str] = &[
    "project", "note", "task", "plan", "link", "repo", "machine", "service",
];

pub fn statuses(kind: &str) -> &'static [&'static str] {
    match kind {
        "project" => &["active", "paused", "idea", "done"],
        "task" => &["todo", "doing", "waiting", "done"],
        "plan" => &["draft", "active", "done"],
        "machine" => &["online", "offline", "unknown"],
        "service" => &["running", "stopped", "failing", "unknown"],
        "repo" => &["active", "archived"],
        _ => &[],
    }
}

fn default_status(kind: &str) -> &'static str {
    match kind {
        "project" | "repo" => "active",
        "task" => "todo",
        "plan" => "draft",
        "machine" | "service" => "unknown",
        _ => "",
    }
}

/// Today / This week / Older, by UTC day. A missing or future day has no group.
pub fn group_day(day: &str, today: &str, week_start: &str) -> &'static str {
    if day.len() != 10 || today.len() != 10 || day > today {
        return "";
    }
    if day == today {
        "Today"
    } else if week_start.len() == 10 && day >= week_start {
        "This week"
    } else {
        "Older"
    }
}

pub fn file_kind(name: &str) -> &'static str {
    let lower = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match lower.as_str() {
        "html" | "htm" => "page",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "mp4" | "webm" | "mov" => "media",
        _ => "file",
    }
}

pub fn media_kind(mime: &str) -> &'static str {
    if mime.starts_with("image/") || mime.starts_with("video/") {
        "media"
    } else {
        "file"
    }
}

/// http(s) links in message text. Trailing punctuation is not part of the link.
pub fn web_links(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &text[i..];
        let start = rest
            .find("https://")
            .or_else(|| rest.find("http://"))
            .map(|n| i + n);
        let Some(start) = start else { break };
        let tail = &text[start..];
        let end = tail
            .find(|c: char| {
                c.is_whitespace() || matches!(c, '<' | '>' | '"' | '`' | ')' | ']' | '*')
            })
            .unwrap_or(tail.len());
        let mut href = tail[..end]
            .trim_end_matches(|c: char| matches!(c, ')' | ',' | '.' | ';' | '!' | '?' | ']'))
            .to_string();
        if href.parse::<url::Url>().is_ok() && !out.contains(&href) {
            out.push(std::mem::take(&mut href));
        }
        i = start + end.max(1);
    }
    out
}

fn detail_host(href: &str) -> String {
    url::Url::parse(href)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default()
}

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
            if !entry.is_file || !entry.name.ends_with(".json") {
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

    pub async fn public_connectors(&self, slug: &str) -> ApiResult<Vec<Value>> {
        Self::require_slug(slug)?;
        let mut rows = vec![json!({
            "name": "bot",
            "url": self.mcp_url,
            "enabled": true,
            "locked": true,
            "hasSecret": true,
        })];
        for c in self.read_connectors(slug).await? {
            rows.push(json!({
                "name": c["name"],
                "url": c["url"],
                "enabled": c["enabled"] == true,
                "locked": false,
                "hasSecret": c["secret"].as_str().is_some_and(|s| !s.is_empty()),
            }));
        }
        Ok(rows)
    }

    async fn read_connectors(&self, slug: &str) -> ApiResult<Vec<Value>> {
        match self.ws.read(&Self::connectors_path(slug)).await {
            Ok(bytes) => {
                let doc: Value = serde_json::from_slice(&bytes)
                    .map_err(|_| ApiError::new(500, "connectors.json is not valid JSON"))?;
                Ok(doc["connectors"].as_array().cloned().unwrap_or_default())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn save_connector(&self, slug: &str, input: &Value) -> ApiResult<Value> {
        Self::require_slug(slug)?;
        let name = input["name"].as_str().unwrap_or("").trim();
        validate_connector_name(name)?;
        let url = input["url"].as_str().unwrap_or("").trim();
        validate_connector_url(url)?;
        let enabled = input["enabled"] != false;
        let secret = input["secret"].as_str().unwrap_or("").trim().to_string();
        let mut all = self.read_connectors(slug).await?;
        if let Some(found) = all.iter_mut().find(|c| c["name"] == name) {
            let kept = found["secret"].as_str().unwrap_or("").to_string();
            found["url"] = json!(url);
            found["enabled"] = json!(enabled);
            if !secret.is_empty() {
                found["secret"] = json!(secret);
            } else if !kept.is_empty() {
                found["secret"] = json!(kept);
            }
        } else {
            all.push(json!({ "name": name, "url": url, "enabled": enabled, "secret": secret }));
        }
        self.write_connectors(slug, &all).await?;
        self.rewrite_harness(slug, &all).await?;
        Ok(json!({ "ok": true }))
    }

    pub async fn delete_connector(&self, slug: &str, name: &str) -> ApiResult<Value> {
        Self::require_slug(slug)?;
        if name == "bot" {
            return Err(ApiError::bad_request("The vault connector stays on."));
        }
        let mut all = self.read_connectors(slug).await?;
        let before = all.len();
        all.retain(|c| c["name"] != name);
        if all.len() == before {
            return Err(ApiError::not_found("no such connector"));
        }
        self.write_connectors(slug, &all).await?;
        // The deleted name is not in the saved list. Pass it as off so the
        // harness files drop it, and so a later session cannot keep it alive.
        let mut for_harness = all.clone();
        for_harness.push(json!({ "name": name, "enabled": false }));
        self.rewrite_harness(slug, &for_harness).await?;
        Ok(json!({ "ok": true }))
    }

    async fn write_connectors(&self, slug: &str, connectors: &[Value]) -> ApiResult<()> {
        let text =
            serde_json::to_string_pretty(&json!({ "connectors": connectors })).unwrap() + "\n";
        self.ws
            .write(&Self::connectors_path(slug), text.as_bytes(), Some(0o600))
            .await?;
        Ok(())
    }

    async fn rewrite_harness(&self, slug: &str, connectors: &[Value]) -> ApiResult<()> {
        let mcp_rel = format!("{slug}/.mcp.json");
        let existing = match self.ws.read(&mcp_rel).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})),
            Err(_) => json!({}),
        };
        let claude_rel = format!("{slug}/.claude/settings.json");
        let claude_existing = match self.ws.read(&claude_rel).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})),
            Err(_) => json!({}),
        };
        let mcp = merge_mcp(&existing, connectors, &self.mcp_url, &self.mcp_token);
        let claude = claude_settings(&claude_existing, connectors);
        let toml = codex_toml(&mcp);
        let mcp_text = serde_json::to_string_pretty(&mcp).unwrap_or_else(|_| "{}".into()) + "\n";
        let claude_text =
            serde_json::to_string_pretty(&claude).unwrap_or_else(|_| "{}".into()) + "\n";
        self.ws
            .write(&mcp_rel, mcp_text.as_bytes(), Some(0o600))
            .await?;
        self.ws
            .write(
                &format!("{slug}/.codex/config.toml"),
                toml.as_bytes(),
                Some(0o600),
            )
            .await?;
        self.ws
            .write(&claude_rel, claude_text.as_bytes(), None)
            .await?;
        Ok(())
    }
}

const SHELF_CAP: usize = 60;

fn is_picture(title: &str, detail: &str) -> bool {
    detail.starts_with("image/")
        || matches!(
            title.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp"
        )
}

/// A Bot's shelf for the panel: pictures, files and links, newest first, nothing else. Records
/// kept by the person or the Bot (notes, tasks, plans and the rest) and connectors stay out.
/// `url` is a gateway path the app prefixes with its address (or a link's own address).
pub fn shelf_from(slug: &str, view: &Value) -> Value {
    let (mut pictures, mut files, mut links) = (Vec::new(), Vec::new(), Vec::new());
    for e in view["entries"].as_array().into_iter().flatten() {
        let id = e["id"].as_str().unwrap_or("");
        let (title, detail) = (e["title"].as_str().unwrap_or(""), e["detail"].as_str().unwrap_or(""));
        let url = if let Some(path) = id.strip_prefix(&format!("file:{slug}/")) {
            format!("/api/bots/{slug}/library/file?path={}", encode_query(path))
        } else if let Some(att) = id.strip_prefix("attachment:") {
            format!("/api/bots/{slug}/attachments/{att}")
        } else if id.starts_with("http://") || id.starts_with("https://") {
            id.to_string()
        } else {
            continue;
        };
        let item = json!({ "id": id, "title": title, "detail": detail, "url": url, "group": e["group"] });
        match e["kind"].as_str().unwrap_or("") {
            "media" if is_picture(title, detail) => pictures.push(item),
            "media" | "file" | "page" => files.push(item),
            "link" => links.push(item),
            _ => {}
        }
    }
    let counts = json!({ "pictures": pictures.len(), "files": files.len(), "links": links.len() });
    pictures.truncate(SHELF_CAP);
    files.truncate(SHELF_CAP);
    links.truncate(SHELF_CAP);
    json!({ "ok": true, "counts": counts, "pictures": pictures, "files": files, "links": links })
}

/// A path as a query value: the characters a path uses stay, the rest are percent-encoded.
fn encode_query(path: &str) -> String {
    path.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'/' | b'~' => (b as char).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn file_entry(slug: &str, entry: DirEntry, today: &str, week: &str) -> Value {
    let day = if entry.modified == 0 {
        String::new()
    } else {
        day_key(entry.modified)
    };
    json!({
        "id": format!("file:{slug}/library/{}", entry.name),
        "kind": file_kind(&entry.name),
        "title": entry.name,
        "detail": if file_kind(&entry.name) == "page" { "Private" } else { "Workspace file" },
        "group": group_day(&day, today, week),
        "status": "",
        "body": "",
    })
}

fn new_id() -> String {
    const ABC: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
    let n = rand::random::<u64>();
    (0..8)
        .map(|i| ABC[((n >> (i * 5)) as usize) % ABC.len()] as char)
        .collect()
}

pub fn validate_connector_name(name: &str) -> ApiResult<()> {
    let ok = !name.is_empty()
        && name.len() <= 32
        && name != "bot"
        && name != "executor"
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "A connector name is a short token, and not bot or executor.",
        ))
    }
}

pub fn validate_connector_url(url: &str) -> ApiResult<()> {
    match url::Url::parse(url) {
        Ok(u)
            if matches!(u.scheme(), "http" | "https") && !u.host_str().unwrap_or("").is_empty() =>
        {
            Ok(())
        }
        _ => Err(ApiError::bad_request(
            "A connector URL starts with http:// or https://.",
        )),
    }
}

/// `bot` stays. An enabled connector is written. A disabled one is removed.
/// A secret never appears in a response; it is only placed in the file the harness reads.
pub fn merge_mcp(existing: &Value, connectors: &[Value], bot_url: &str, bot_token: &str) -> Value {
    let mut mcp = existing.clone();
    if !mcp.is_object() {
        mcp = json!({});
    }
    let mut servers = mcp["mcpServers"].as_object().cloned().unwrap_or_default();
    servers.insert(
        "bot".into(),
        json!({
            "type": "http",
            "url": bot_url,
            "headers": { "Authorization": format!("Bearer {bot_token}") },
        }),
    );
    for c in connectors {
        let Some(name) = c["name"].as_str() else {
            continue;
        };
        if name == "bot" || name == "executor" {
            continue;
        }
        if c["enabled"] == true {
            let mut row = json!({ "type": "http", "url": c["url"] });
            if let Some(secret) = c["secret"].as_str().filter(|s| !s.is_empty()) {
                row["headers"] = json!({ "Authorization": format!("Bearer {secret}") });
            }
            servers.insert(name.to_string(), row);
        } else {
            servers.remove(name);
        }
    }
    mcp["mcpServers"] = json!(servers);
    mcp
}

pub fn claude_settings(existing: &Value, connectors: &[Value]) -> Value {
    let mut cs = if existing.is_object() {
        existing.clone()
    } else {
        json!({})
    };
    cs["enableAllProjectMcpServers"] = json!(true);
    let disabled: Vec<&str> = connectors
        .iter()
        .filter(|c| c["enabled"] != true)
        .filter_map(|c| c["name"].as_str())
        .filter(|n| *n != "bot")
        .collect();
    cs["disabledMcpjsonServers"] = json!(disabled);
    cs
}

pub fn codex_toml(mcp: &Value) -> String {
    let mut out =
        String::from("# Written by the Bot gateway. bot = the Vault and other Bot tools.\n");
    if let Some(servers) = mcp["mcpServers"].as_object() {
        let mut names: Vec<_> = servers.keys().cloned().collect();
        names.sort();
        for name in names {
            let s = &servers[&name];
            let Some(url) = s["url"].as_str() else {
                continue;
            };
            out.push_str(&format!("[mcp_servers.{name}]\nurl = {url:?}\n"));
            if let Some(auth) = s["headers"]["Authorization"].as_str() {
                out.push_str(&format!(
                    "http_headers = {{ \"Authorization\" = {auth:?} }}\n"
                ));
            }
            out.push_str("tool_timeout_sec = 960\n");
        }
    }
    out
}

pub fn encode_file(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::MemoryWorkspace;

    fn lib() -> Library {
        Library::new(
            Arc::new(MemoryWorkspace::new()),
            "http://127.0.0.1:3780/mcp/bot".into(),
            "tok".into(),
        )
    }

    #[test]
    fn groups_and_kinds() {
        assert_eq!(group_day("2026-11-10", "2026-11-10", "2026-11-04"), "Today");
        assert_eq!(
            group_day("2026-11-08", "2026-11-10", "2026-11-04"),
            "This week"
        );
        assert_eq!(group_day("2026-01-01", "2026-11-10", "2026-11-04"), "Older");
        assert_eq!(group_day("2026-12-01", "2026-11-10", "2026-11-04"), "");
        assert_eq!(group_day("", "2026-11-10", "2026-11-04"), "");
        assert_eq!(file_kind("Report.html"), "page");
        assert_eq!(file_kind("shot.PNG"), "media");
        assert_eq!(file_kind("notes.txt"), "file");
        assert_eq!(media_kind("image/png"), "media");
        assert_eq!(media_kind("application/pdf"), "file");
    }

    #[test]
    fn links_drop_junk_and_duplicates() {
        let found =
            web_links("See https://example.com/a), and https://example.com/a again. Not ftp://x");
        assert_eq!(found, vec!["https://example.com/a".to_string()]);
    }

    #[tokio::test]
    async fn records_round_trip_through_the_workspace() {
        let library = lib();
        library
            .save_item("ada", &json!({ "kind": "plan", "title": "Week" }), 0)
            .await
            .unwrap();
        library
            .save_item("ada", &json!({ "kind": "note", "title": "Milk" }), 0)
            .await
            .unwrap();
        library
            .save_item(
                "ada",
                &json!({ "kind": "task", "title": "Call", "status": "doing" }),
                0,
            )
            .await
            .unwrap();
        library
            .save_item("ada", &json!({ "kind": "project", "title": "Home" }), 0)
            .await
            .unwrap();
        let err = library
            .save_item(
                "ada",
                &json!({ "kind": "plan", "title": "Bad", "status": "nope" }),
                0,
            )
            .await;
        assert!(err.is_err());
        let view = library
            .view(
                "ada",
                &[json!({ "text": "https://example.com/x", "at": "2026-11-10T00:00:00Z" })],
                1_762_732_800, // 2026-11-10
            )
            .await
            .unwrap();
        let kinds: Vec<_> = view["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["kind"].as_str().unwrap())
            .collect();
        assert!(kinds.contains(&"plan"));
        assert!(kinds.contains(&"note"));
        assert!(kinds.contains(&"task"));
        assert!(kinds.contains(&"project"));
        assert!(kinds.contains(&"link"));
        let plan = view["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "plan")
            .unwrap();
        assert_eq!(plan["status"], "draft");
        let id = plan["id"].as_str().unwrap();
        library.delete_item("ada", id).await.unwrap();
        let again = library.view("ada", &[], 1_762_732_800).await.unwrap();
        assert!(again["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["id"] != id));
        let raw = library.ws.read("ada/.bot/library.json").await.unwrap();
        assert!(raw.starts_with(b"{"));
    }

    #[tokio::test]
    async fn files_stay_inside_library() {
        let library = lib();
        library
            .ws
            .write("ada/library/page.html", b"<p>kept</p>", None)
            .await
            .unwrap();
        library
            .ws
            .write("ada/secret.txt", b"no", None)
            .await
            .unwrap();
        let view = library.view("ada", &[], 1_700_000_000).await.unwrap();
        let titles: Vec<_> = view["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["title"].as_str().unwrap())
            .collect();
        assert_eq!(titles, vec!["page.html"]);
        assert_eq!(
            library
                .file_bytes("ada", "library/page.html")
                .await
                .unwrap(),
            b"<p>kept</p>"
        );
        assert!(library.file_bytes("ada", "../secret.txt").await.is_err());
        assert!(library.file_bytes("ada", "secret.txt").await.is_err());
        assert!(library.view("../ada", &[], 0).await.is_err());
    }

    #[tokio::test]
    async fn connector_switch_rewrites_harness_files_and_hides_the_secret() {
        let library = lib();
        library
            .save_connector(
                "ada",
                &json!({ "name": "linear", "url": "https://mcp.example/linear", "enabled": true, "secret": "s3cret" }),
            )
            .await
            .unwrap();
        let view = library.view("ada", &[], 0).await.unwrap();
        let shown = &view["connectors"];
        let linear = shown
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "linear")
            .unwrap();
        assert_eq!(linear["enabled"], true);
        assert_eq!(linear["hasSecret"], true);
        assert!(linear.get("secret").is_none());
        let bot = shown
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "bot")
            .unwrap();
        assert_eq!(bot["locked"], true);
        let mcp: Value =
            serde_json::from_slice(&library.ws.read("ada/.mcp.json").await.unwrap()).unwrap();
        assert_eq!(
            mcp["mcpServers"]["linear"]["url"],
            "https://mcp.example/linear"
        );
        assert!(mcp["mcpServers"]["bot"]["url"]
            .as_str()
            .unwrap()
            .contains("/mcp/bot"));
        let toml =
            String::from_utf8(library.ws.read("ada/.codex/config.toml").await.unwrap()).unwrap();
        assert!(toml.contains("[mcp_servers.linear]"));
        assert!(toml.contains("[mcp_servers.bot]"));
        library
            .save_connector(
                "ada",
                &json!({ "name": "linear", "url": "https://mcp.example/linear", "enabled": false }),
            )
            .await
            .unwrap();
        let mcp: Value =
            serde_json::from_slice(&library.ws.read("ada/.mcp.json").await.unwrap()).unwrap();
        assert!(mcp["mcpServers"].get("linear").is_none());
        assert!(mcp["mcpServers"].get("bot").is_some());
        let claude: Value =
            serde_json::from_slice(&library.ws.read("ada/.claude/settings.json").await.unwrap())
                .unwrap();
        assert_eq!(claude["disabledMcpjsonServers"][0], "linear");
        assert!(library
            .save_connector(
                "ada",
                &json!({ "name": "executor", "url": "https://x", "enabled": true }),
            )
            .await
            .is_err());
        assert!(library
            .save_connector(
                "ada",
                &json!({ "name": "ok", "url": "ftp://x", "enabled": true }),
            )
            .await
            .is_err());
    }

    #[test]
    fn merge_never_prints_a_disabled_secret_into_mcp() {
        let mcp = merge_mcp(
            &json!({}),
            &[
                json!({ "name": "linear", "url": "https://mcp.example/a", "enabled": false, "secret": "s3cret" }),
            ],
            "http://127.0.0.1/mcp/bot",
            "tok",
        );
        let text = mcp.to_string();
        assert!(!text.contains("s3cret"));
        assert!(!text.contains("linear"));
        assert!(text.contains("bot"));
    }

    #[tokio::test]
    async fn every_board_kind_gets_its_default_and_a_note_has_none() {
        let library = lib();
        for (kind, status) in [
            ("project", "active"),
            ("task", "todo"),
            ("plan", "draft"),
            ("machine", "unknown"),
            ("service", "unknown"),
            ("repo", "active"),
            ("link", ""),
            ("note", ""),
        ] {
            library
                .save_item("ada", &json!({ "kind": kind, "title": kind }), 0)
                .await
                .unwrap();
            let view = library.view("ada", &[], 0).await.unwrap();
            let row = view["entries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["kind"] == kind)
                .unwrap();
            assert_eq!(row["status"], status, "{kind}");
        }
        library
            .save_item(
                "ada",
                &json!({ "kind": "note", "title": "Aside", "status": "active" }),
                0,
            )
            .await
            .unwrap();
        let view = library.view("ada", &[], 0).await.unwrap();
        let aside = view["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["title"] == "Aside")
            .unwrap();
        assert_eq!(aside["status"], "");
        assert!(library
            .save_item("ada", &json!({ "kind": "genealogy", "title": "No" }), 0)
            .await
            .is_err());
        assert!(library
            .save_item("ada", &json!({ "kind": "note", "title": "  " }), 0)
            .await
            .is_err());
        assert!(library
            .save_item(
                "ada",
                &json!({ "id": "missing", "kind": "note", "title": "Later" }),
                0
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn an_empty_secret_keeps_the_token_and_delete_drops_the_server() {
        let library = lib();
        library
            .save_connector(
                "ada",
                &json!({ "name": "notes", "url": "http://127.0.0.1:9/mcp", "enabled": true, "secret": "s3cret" }),
            )
            .await
            .unwrap();
        library
            .save_connector(
                "ada",
                &json!({ "name": "notes", "url": "https://mcp.example/notes", "enabled": true, "secret": "" }),
            )
            .await
            .unwrap();
        let shown = library.public_connectors("ada").await.unwrap();
        let notes = shown.iter().find(|c| c["name"] == "notes").unwrap();
        assert_eq!(notes["hasSecret"], true);
        assert_eq!(notes["url"], "https://mcp.example/notes");
        assert!(notes.get("secret").is_none());
        let mcp: Value =
            serde_json::from_slice(&library.ws.read("ada/.mcp.json").await.unwrap()).unwrap();
        assert!(mcp["mcpServers"]["notes"]["headers"]["Authorization"]
            .as_str()
            .unwrap()
            .contains("s3cret"));
        library.delete_connector("ada", "notes").await.unwrap();
        let mcp: Value =
            serde_json::from_slice(&library.ws.read("ada/.mcp.json").await.unwrap()).unwrap();
        assert!(mcp["mcpServers"].get("notes").is_none());
        assert!(mcp["mcpServers"].get("bot").is_some());
        let toml =
            String::from_utf8(library.ws.read("ada/.codex/config.toml").await.unwrap()).unwrap();
        assert!(!toml.contains("notes"));
        assert!(!toml.contains("s3cret"));
        let claude: Value =
            serde_json::from_slice(&library.ws.read("ada/.claude/settings.json").await.unwrap())
                .unwrap();
        assert_eq!(claude["disabledMcpjsonServers"], json!(["notes"]));
        assert!(library.delete_connector("ada", "bot").await.is_err());
        assert!(library.delete_connector("ada", "notes").await.is_err());
    }

    #[test]
    fn connector_names_and_urls_are_short_http_tokens() {
        assert!(validate_connector_name("bot").is_err());
        assert!(validate_connector_name("executor").is_err());
        assert!(validate_connector_name("").is_err());
        assert!(validate_connector_name("has space").is_err());
        assert!(validate_connector_name(&"a".repeat(33)).is_err());
        assert!(validate_connector_name(&"a".repeat(32)).is_ok());
        assert!(validate_connector_name("linear_1").is_ok());
        assert!(validate_connector_url("http://127.0.0.1:9/mcp").is_ok());
        assert!(validate_connector_url("https://mcp.example/a").is_ok());
        assert!(validate_connector_url("ftp://mcp.example/a").is_err());
        assert!(validate_connector_url("https://").is_err());
        assert!(validate_connector_url("").is_err());
    }

    #[tokio::test]
    async fn paths_outside_library_are_refused() {
        let library = lib();
        library
            .ws
            .write("ada/library/page.html", b"<p>kept</p>", None)
            .await
            .unwrap();
        assert!(library
            .file_bytes("ada", "library/../secret.txt")
            .await
            .is_err());
        assert!(library.file_bytes("ada", "").await.is_err());
        assert!(library.file_bytes("ada", ".").await.is_err());
        assert!(library
            .file_bytes("ada", "library/missing.html")
            .await
            .is_err());
        assert_eq!(
            library
                .file_bytes("ada", "/library/page.html")
                .await
                .unwrap(),
            b"<p>kept</p>"
        );
    }

    #[test]
    fn harness_files_sort_servers_and_disable_only_what_is_off() {
        let mcp = merge_mcp(
            &json!({}),
            &[
                json!({ "name": "zebra", "url": "https://mcp.example/z", "enabled": true, "secret": "z" }),
                json!({ "name": "notes", "url": "https://mcp.example/n", "enabled": false, "secret": "hidden" }),
                json!({ "name": "bot", "url": "https://nope", "enabled": false, "secret": "no" }),
            ],
            "http://127.0.0.1/mcp/bot",
            "tok",
        );
        let toml = codex_toml(&mcp);
        let bot_at = toml.find("[mcp_servers.bot]").unwrap();
        let zebra_at = toml.find("[mcp_servers.zebra]").unwrap();
        assert!(bot_at < zebra_at);
        assert!(!toml.contains("notes"));
        assert!(!toml.contains("hidden"));
        assert!(toml.contains("tool_timeout_sec = 960"));
        assert!(toml.contains("https://mcp.example/z"));
        let claude = claude_settings(
            &json!({}),
            &json!([
                { "name": "notes", "enabled": false },
                { "name": "zebra", "enabled": true },
                { "name": "bot", "enabled": false }
            ])
            .as_array()
            .unwrap(),
        );
        assert_eq!(claude["enableAllProjectMcpServers"], true);
        assert_eq!(claude["disabledMcpjsonServers"], json!(["notes"]));
    }

    #[test]
    fn the_shelf_keeps_pictures_files_and_links_and_drops_records_and_connectors() {
        let view = json!({ "entries": [
            { "id": "attachment:a1", "kind": "media", "title": "chart.png", "detail": "image/png", "group": "Today" },
            { "id": "attachment:a2", "kind": "media", "title": "clip.mp4", "detail": "video/mp4", "group": "Today" },
            { "id": "file:ada/library/Notes today.html", "kind": "page", "title": "Notes today.html", "detail": "Private", "group": "Today" },
            { "id": "file:ada/library/plan.pdf", "kind": "file", "title": "plan.pdf", "detail": "Workspace file", "group": "Older" },
            { "id": "https://example.com/a", "kind": "link", "title": "example.com", "detail": "https://example.com/a", "group": "Today" },
            { "id": "abc123", "kind": "task", "title": "Ship it", "detail": "doing", "group": "Today" },
            { "id": "xyz", "kind": "note", "title": "A note", "detail": "", "group": "Today" },
        ], "connectors": [{ "name": "notes" }] });
        let shelf = shelf_from("ada", &view);
        assert_eq!(shelf["counts"], json!({ "pictures": 1, "files": 3, "links": 1 }));
        assert_eq!(shelf["pictures"][0]["url"], "/api/bots/ada/attachments/a1");
        assert_eq!(shelf["files"][1]["url"], "/api/bots/ada/library/file?path=library/Notes%20today.html");
        assert_eq!(shelf["links"][0]["url"], "https://example.com/a");
        assert!(shelf.get("connectors").is_none());
        assert!(!shelf.to_string().contains("Ship it"));
    }

}
