//! A watch reacts to a change with no model cost per poll (`docs/webhook-triggers-plan.md`,
//! Phase 4). The store is `bots/<slug>/.bot/watches.json`, written mode 600. Targets are
//! an HTTP GET or HEAD, or a file under the Bot folder read through the Workspace.
//! Checks are evaluated against the stored state; a fire POSTs to the watch's hidden
//! webhook routine on this machine's own T3 (`routine.rs`, `poller.rs`).
//!
//! `allowPrivate` is a product decision: it is set only from the app UI, after a
//! confirmation, and never from an MCP tool (`watch_create` refuses private hosts).

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub(crate) mod evaluate;
pub(crate) mod guard;
pub(crate) mod poller;
pub(crate) mod routine;

#[cfg(test)]
mod tests;

/// The same floor T3 uses for intervals.
pub(crate) const MIN_EVERY_MS: u64 = 60_000;
/// The Bots' own tools poll no faster than this.
pub(crate) const TOOL_MIN_EVERY_MS: u64 = 300_000;
pub(crate) const MIN_COOLDOWN_S: u64 = 60;
/// At most this many watches per Bot, from any source.
pub(crate) const MAX_PER_BOT: usize = 10;

/// One mutation of a watch file at a time, per process. Watch writes are rare, and
/// the guard is async so a save can hold it across the file write.
static WRITE_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

/// `bots/<slug>/.bot/watches.json`.
#[derive(Clone, Debug)]
pub(crate) struct WatchFile {
    path: PathBuf,
    /// The relative path a `file` target is stored under: `<slug>/...`.
    slug: String,
}

impl WatchFile {
    pub(crate) fn new(bots_root: &Path, slug: &str) -> Self {
        Self {
            path: bots_root.join(slug).join(".bot").join("watches.json"),
            slug: slug.to_string(),
        }
    }

    /// Empty when missing. A corrupt file recovers to empty with a warning; `watches.json`
    /// is inert without the poller, so dropping it is safe.
    pub(crate) async fn load(&self) -> Value {
        match crate::storage::read_json(&self.path).await {
            Some(doc) => doc,
            None if !self.path.exists() => empty_doc(),
            None => {
                tracing::warn!("{} is unreadable; watches start empty", self.path.display());
                empty_doc()
            }
        }
    }

    /// Write the whole document, mode 600. One writer at a time.
    pub(crate) async fn save(&self, doc: &Value) -> Result<(), String> {
        let held = self.lock().await;
        let result = crate::storage::write_json_private(&self.path, doc)
            .await
            .map_err(|err| err.to_string());
        drop(held);
        result
    }

    pub(crate) async fn lock(&self) -> tokio::sync::MutexGuard<'static, ()> {
        WRITE_LOCK.lock().await
    }

    pub(crate) fn watches(&self, doc: &Value) -> Vec<Value> {
        doc["watches"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|w| w["id"].is_string())
            .collect()
    }

    /// Load, run `edit` under the write lock, save. `Err` keeps the file unchanged.
    pub(crate) async fn update<F>(&self, edit: F) -> Result<Vec<Value>, String>
    where
        F: FnOnce(&mut Vec<Value>) -> Result<(), String>,
    {
        let held = self.lock().await;
        let mut doc = match crate::storage::read_json(&self.path).await {
            Some(doc) => doc,
            None if !self.path.exists() => empty_doc(),
            None => {
                tracing::warn!("{} is unreadable; watches start empty", self.path.display());
                empty_doc()
            }
        };
        let mut watches = self.watches(&doc);
        edit(&mut watches)?;
        doc = json!({ "version": 1, "watches": watches });
        crate::storage::write_json_private(&self.path, &doc)
            .await
            .map_err(|err| err.to_string())?;
        drop(held);
        Ok(doc["watches"].as_array().cloned().unwrap_or_default())
    }

    /// A relative path under this Bot's folder, as the Workspace sees it.
    pub(crate) fn workspace_rel(&self, rel: &str) -> Result<String, String> {
        let rel = rel.trim().trim_start_matches('/');
        crate::storage::workspace::safe_rel(rel)
            .map_err(|_bad| "A file target stays inside this Bot's folder.".to_string())?;
        if !rel.starts_with(&format!("{}/", self.slug)) {
            // The tools store the path without the slug; keep the Workspace key absolute.
            return Ok(format!("{}/{}", self.slug, rel));
        }
        Ok(rel.to_string())
    }
}

pub(crate) fn empty_doc() -> Value {
    json!({ "version": 1, "watches": [] })
}

/// A watch id: `w_` plus ten base-36 characters.
pub(crate) fn new_id() -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let raw: [u8; 10] = std::array::from_fn(|_| rand::random::<u8>());
    format!(
        "w_{}",
        raw.iter()
            .map(|b| ALPHABET[(b % 36) as usize] as char)
            .collect::<String>()
    )
}

/// Milliseconds for [`routine.rs`]: the cooldown's coalesce window, capped at T3's range.
pub(crate) fn coalesce_seconds(cooldown_sec: u64) -> u64 {
    cooldown_sec.clamp(10, 3600)
}

/// Validate the request shape shared by the tool and the app sheet.
/// `check` is `{ type, op?, pointer?, value?, substring?, regex? }`.
pub(crate) fn validate_check(check: &Value) -> Result<Value, &'static str> {
    let Some(kind) = check.get("type").and_then(Value::as_str) else {
        return Err("A watch needs a check: hash, status, json, contains, or regex.");
    };
    let op = |value: &Value| {
        value
            .get("op")
            .and_then(Value::as_str)
            .unwrap_or("changes")
            .to_string()
    };
    match kind {
        "hash" | "status" => Ok(json!({ "type": kind, "op": "changes" })),
        "json" => {
            let pointer = check
                .get("pointer")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|p| !p.is_empty() && p.starts_with('/'))
                .ok_or("A json check needs a pointer such as /state.")?;
            let mut out = json!({ "type": "json", "op": op(check), "pointer": pointer });
            if out["op"].as_str() == Some("changes") {
                return Ok(json!({ "type": "json", "op": "changes", "pointer": pointer }));
            }
            if !matches!(
                out["op"].as_str(),
                Some("equals") | Some("not_equals") | Some("gt") | Some("lt")
            ) {
                return Err("A json op is changes, equals, not_equals, gt, or lt.");
            }
            if let Some(value) = check.get("value") {
                out["value"] = value.clone();
            } else {
                return Err("equals, not_equals, gt, and lt need a value.");
            }
            Ok(out)
        }
        "contains" => {
            let substring = check
                .get("substring")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or("A contains check needs a substring.")?;
            Ok(json!({ "type": "contains", "op": "changes", "substring": substring }))
        }
        "regex" => {
            let pattern = check
                .get("regex")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or("A regex check needs a pattern.")?;
            if pattern.len() > 512 {
                return Err("A regex pattern is at most 512 characters.");
            }
            let compiled = regex::RegexBuilder::new(pattern)
                .size_limit(crate::bots::watch::evaluate::REGEX_SIZE_LIMIT)
                .build()
                .map_err(|_bad| "That regex does not compile.")?;
            if compiled.as_str().is_empty() {
                return Err("That regex matches everything.");
            }
            Ok(json!({ "type": "regex", "op": "changes", "regex": pattern }))
        }
        _ => Err("A watch needs a check: hash, status, json, contains, or regex."),
    }
}

/// `fire` is `{ mode, cooldownSec }`: `on_change` (default), `on_true`, or `every`.
pub(crate) fn validate_fire(fire: &Value) -> Result<Value, &'static str> {
    let mode = fire
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("on_change");
    if !matches!(mode, "on_change" | "on_true" | "every") {
        return Err("Fire mode is on_change, on_true, or every.");
    }
    let cooldown = match fire.get("cooldownSec") {
        None | Some(Value::Null) => 300,
        Some(value) => value
            .as_u64()
            .ok_or("Cooldown is a number of seconds.")?
            .max(MIN_COOLDOWN_S),
    };
    Ok(json!({ "mode": mode, "cooldownSec": cooldown }))
}

/// `target` is `{ type: "http", url, method }` or `{ type: "file", path }`.
pub(crate) fn validate_target(target: &Value) -> Result<Value, &'static str> {
    match target.get("type").and_then(Value::as_str) {
        Some("http") => {
            let url = target
                .get("url")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|u| !u.is_empty())
                .ok_or("An http watch needs a url.")?;
            let parsed = url::Url::parse(url).map_err(|_bad| "That url does not parse.")?;
            if !matches!(parsed.scheme(), "http" | "https") {
                return Err("A watch url is http or https.");
            }
            let _ = parsed
                .host_str()
                .filter(|h| !h.is_empty())
                .ok_or("A watch url needs a host.")?;
            let method = match target.get("method").and_then(Value::as_str) {
                None | Some("GET") => "GET",
                Some("HEAD") => "HEAD",
                Some(_) => return Err("An http watch is GET or HEAD."),
            };
            Ok(json!({ "type": "http", "url": url, "method": method }))
        }
        Some("file") => {
            let path = target
                .get("path")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .ok_or("A file watch needs a path inside this Bot's folder.")?;
            crate::storage::workspace::safe_rel(path)
                .map_err(|_bad| "A file target stays inside this Bot's folder.")?;
            Ok(json!({ "type": "file", "path": path }))
        }
        Some(_) => Err("A watch target is http or file."),
        None => Err("A watch needs a target."),
    }
}

/// The `everyMs` for a poll. `floor` is the caller's minimum (tool: 5 minutes).
pub(crate) fn validate_every_ms(every_ms: &Value, floor: u64) -> Result<u64, &'static str> {
    let ms = every_ms
        .as_u64()
        .ok_or("A watch repeats every so many milliseconds.")?;
    if ms < floor {
        return Err("A watch repeats no faster than the caller's floor.");
    }
    Ok(ms.max(MIN_EVERY_MS))
}

/// Nothing about a watch row exposes the token: the hidden routine keeps it, and
/// the poller reads the address from the task row at fire time only.
pub(crate) fn row(watch: &Value) -> Value {
    let state = &watch["state"];
    json!({
        "id": watch["id"],
        "name": watch["name"],
        "enabled": watch["enabled"] == true,
        "paused": watch["enabled"] != true,
        "target": target_label(watch),
        "targetType": watch["target"]["type"],
        "everyMs": watch["everyMs"],
        "schedule": schedule_label(watch),
        "nextRunAt": state["nextAt"],
        "lastRunAt": state["lastAt"],
        "lastStatus": if state["lastError"].as_str().unwrap_or("").is_empty() { "ok" } else { "failing" },
        "lastError": state["lastError"],
        "blocked": false,
        "kind": "watch",
        "watch": watch_summary(watch),
    })
}

/// A new watch record: enabled, its state empty, and no hidden routine yet. The
/// routine is upserted next, and its id filled in.
pub(crate) fn new_record(
    name: &str,
    target: &Value,
    check: &Value,
    fire: &Value,
    every_ms: u64,
) -> Value {
    json!({
        "id": new_id(),
        "name": name,
        "enabled": true,
        "target": target,
        "check": check,
        "fire": fire,
        "everyMs": every_ms,
        "allowPrivate": false,
        "routineId": "",
        "task": Value::Null,
        "state": {
            "last": "",
            "lastTrue": Value::Null,
            "lastAt": "",
            "lastHash": "",
            "lastError": "",
            "failures": 0,
            "failingSince": 0,
            "lastFireAt": 0,
            "nextAt": poller::now_ms(),
        }
    })
}

/// Upsert the hidden webhook routine (`routine.rs`) and fill the watch from the saved
/// task: `routineId`, and the task body itself so a routine T3 loses can be recreated.
pub(crate) async fn hidden_routine(
    state: &crate::AppState,
    routines: &dyn crate::harness::Routines,
    bot: &Value,
    watch: &mut Value,
) -> Result<(), String> {
    let task = routine::hidden_task(bot, watch).map_err(str::to_string)?;
    let saved = crate::chat::routines::change(routines, &state.routine_history, "upsert", task)
        .await
        .map_err(|err| err.message)?;
    let saved_task = saved.get("task").cloned().unwrap_or_else(|| saved.clone());
    let id = saved_task["id"].as_str().unwrap_or("").to_string();
    if id.is_empty() {
        return Err("T3 did not create the watch's routine.".to_string());
    }
    watch["routineId"] = json!(id);
    watch["task"] = saved_task;
    Ok(())
}

/// Delete the hidden routine with its watch. T3 lost it already: that is a success.
pub(crate) async fn forget_routine(
    state: &crate::AppState,
    routine_id: &str,
) -> Result<(), String> {
    if routine_id.is_empty() {
        return Ok(());
    }
    let routines = state.harness.routines().map_err(|err| err.to_string())?;
    let deleted = crate::chat::routines::change(
        routines,
        &state.routine_history,
        "delete",
        json!({ "id": routine_id }),
    )
    .await;
    match deleted {
        Ok(_) => Ok(()),
        Err(err) if err.message.contains("not found") => Ok(()),
        Err(err) => Err(err.message),
    }
}

fn watch_summary(watch: &Value) -> Value {
    json!({
        "allowPrivate": watch["allowPrivate"] == true,
        "failing": watch["state"]["failures"].as_u64().unwrap_or(0),
        "mode": watch["fire"]["mode"],
        "cooldownSec": watch["fire"]["cooldownSec"],
        "checkType": watch["check"]["type"],
        "last": watch["state"]["last"],
    })
}

fn target_label(watch: &Value) -> String {
    let target = &watch["target"];
    match target["type"].as_str() {
        Some("file") => target["path"].as_str().unwrap_or("").to_string(),
        _ => url::Url::parse(target["url"].as_str().unwrap_or(""))
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .unwrap_or_default(),
    }
}

fn schedule_label(watch: &Value) -> String {
    let minutes = watch["everyMs"].as_u64().unwrap_or(0) / 60_000;
    let base = if minutes > 0 && minutes < 90 {
        format!("Watch, every {minutes} min")
    } else {
        "Watch".to_string()
    };
    if watch["enabled"] == true {
        base
    } else {
        format!("{base}, paused")
    }
}
