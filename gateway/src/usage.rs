//! Local usage counts in `bots/.botapp/usage.json`.
//!
//! The gateway counts what it did (messages, attachments, vault writes, icon fetches) and
//! keeps that on disk. Nothing is sent anywhere. A remote telemetry destination is not
//! in this repo.

use crate::error::{ApiError, ApiResult};
use crate::AppState;
use axum::{extract::State, Json};
use serde_json::{json, Map, Value};
use std::{path::PathBuf, sync::Arc, time::SystemTime};
use tokio::sync::Mutex;

const KEEP_DAYS: usize = 400;

#[derive(Clone, Copy)]
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

pub struct Usage {
    path: PathBuf,
    lock: Mutex<()>,
}

impl Usage {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
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
        crate::store::write_json(&self.path, &doc).await?;
        Ok(())
    }

    pub async fn snapshot(&self) -> ApiResult<Value> {
        let _g = self.lock.lock().await;
        let doc = read_doc(&self.path).await?;
        Ok(with_totals(doc))
    }

    /// Messages and attachments this Bot sent through the gateway.
    pub async fn bot_activity(&self, slug: &str) -> ApiResult<(u64, u64)> {
        let _g = self.lock.lock().await;
        let doc = read_doc(&self.path).await?;
        Ok(bot_counts(&doc, slug))
    }
}

/// Token totals T3 reported on a thread projection. Cache figures are separate
/// from input and output, because T3 counts them that way.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct TokenTotals {
    pub turns: u64,
    pub input: u64,
    pub output: u64,
    pub cached: u64,
    pub cache_write: u64,
}

/// Sum `providerTurns[].turnTokenUsage`. A turn with no usage object means this
/// T3 did not report tokens. `unavailable` and local-command turns are seen, and not added.
pub fn tokens_from_projection(projection: &Value) -> (TokenTotals, bool) {
    let mut totals = TokenTotals::default();
    let mut known = false;
    let Some(turns) = projection.get("providerTurns").and_then(Value::as_array) else {
        return (totals, false);
    };
    for turn in turns {
        let Some(usage) = turn.get("turnTokenUsage").filter(|u| u.is_object()) else {
            continue;
        };
        known = true;
        if usage.get("usageStatus").and_then(Value::as_str) == Some("unavailable")
            || usage.get("localCommand") == Some(&Value::Bool(true))
        {
            continue;
        }
        totals.turns += 1;
        totals.input += count_u64(&usage["inputTokens"]);
        totals.output += count_u64(&usage["outputTokens"]);
        totals.cached += count_u64(&usage["cachedInputTokens"]);
        totals.cache_write += count_u64(&usage["cacheCreationTokens"]);
    }
    (totals, known)
}

pub fn bot_payload(
    messages: u64,
    attachments: u64,
    tokens: &TokenTotals,
    tokens_known: bool,
    error: &str,
) -> Value {
    json!({
        "ok": true,
        "error": error,
        "messages": messages,
        "attachments": attachments,
        "turns": tokens.turns,
        "inputTokens": tokens.input,
        "outputTokens": tokens.output,
        "cachedInputTokens": tokens.cached,
        "cacheCreationTokens": tokens.cache_write,
        "tokensKnown": tokens_known,
    })
}

pub async fn get_usage(State(s): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    Ok(Json(s.usage.snapshot().await?))
}

async fn read_doc(path: &std::path::Path) -> ApiResult<Value> {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => {
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
pub fn record_for(doc: &mut Value, day: &str, slug: &str, key: &str, keep: usize) {
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

pub fn bot_counts(doc: &Value, slug: &str) -> (u64, u64) {
    let mut messages = 0;
    let mut attachments = 0;
    let Some(days) = doc.get("days").and_then(Value::as_object) else {
        return (0, 0);
    };
    for day in days.values() {
        let Some(bucket) = day
            .get("bots")
            .and_then(|b| b.get(slug))
            .and_then(Value::as_object)
        else {
            continue;
        };
        messages += bucket.get("messages").and_then(Value::as_u64).unwrap_or(0);
        attachments += bucket
            .get("attachments")
            .and_then(Value::as_u64)
            .unwrap_or(0);
    }
    (messages, attachments)
}

fn count_u64(value: &Value) -> u64 {
    value.as_u64().unwrap_or(0)
}

/// Add one count on `day` and drop the oldest days past `keep`.
pub fn record(doc: &mut Value, day: &str, key: &str, keep: usize) {
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

pub fn with_totals(mut doc: Value) -> Value {
    let mut totals = Map::new();
    if let Some(days) = doc["days"].as_object() {
        for day in days.values() {
            if let Some(obj) = day.as_object() {
                for (k, v) in obj {
                    let Some(add) = v.as_u64() else { continue };
                    let n = totals.get(k).and_then(Value::as_u64).unwrap_or(0);
                    totals.insert(k.clone(), json!(n + add));
                }
            }
        }
    }
    doc["totals"] = Value::Object(totals);
    doc
}

pub fn day_key(unix_secs: u64) -> String {
    // Howard Hinnant, civil_from_days. UTC, no leap seconds.
    let z = (unix_secs / 86_400) as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_utc_days() {
        assert_eq!(day_key(0), "1970-01-01");
        assert_eq!(day_key(86_400), "1970-01-02");
        assert_eq!(day_key(1_582_934_400), "2020-02-29");
        assert_eq!(day_key(1_794_268_800), "2026-11-10");
    }

    #[test]
    fn counts_add_up_and_old_days_drop() {
        let mut doc = json!({ "version": 1, "days": {} });
        record(&mut doc, "2026-11-10", "messages", 3);
        record(&mut doc, "2026-11-10", "messages", 3);
        record(&mut doc, "2026-11-09", "vaultWrites", 3);
        assert_eq!(doc["days"]["2026-11-10"]["messages"], 2);
        let view = with_totals(doc.clone());
        assert_eq!(view["totals"]["messages"], 2);
        assert_eq!(view["totals"]["vaultWrites"], 1);
        for n in 0..5 {
            record(&mut doc, &format!("2026-01-{n:02}"), "messages", 2);
        }
        assert_eq!(doc["days"].as_object().unwrap().len(), 2);
        assert!(doc["days"].get("2026-01-00").is_none());
    }

    #[tokio::test]
    async fn a_note_lands_on_disk_and_a_broken_file_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.json");
        let usage = Usage::new(path.clone());
        usage
            .note_at_for("", Event::Message, 1_794_268_800)
            .await
            .unwrap();
        usage
            .note_at_for("", Event::IconFetch, 1_794_268_800)
            .await
            .unwrap();
        let snap = usage.snapshot().await.unwrap();
        assert_eq!(snap["days"]["2026-11-10"]["messages"], 1);
        assert_eq!(snap["totals"]["iconFetches"], 1);
        tokio::fs::write(&path, "{").await.unwrap();
        assert!(usage.note_at_for("", Event::Message, 0).await.is_err());
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), "{");
    }

    #[test]
    fn one_bots_messages_do_not_land_on_another() {
        let mut doc = json!({ "version": 1, "days": {} });
        record_for(&mut doc, "2026-11-10", "ada", "messages", 400);
        record_for(&mut doc, "2026-11-10", "ada", "attachments", 400);
        record_for(&mut doc, "2026-11-09", "milo", "messages", 400);
        assert_eq!(bot_counts(&doc, "ada"), (1, 1));
        assert_eq!(bot_counts(&doc, "milo"), (1, 0));
        assert_eq!(bot_counts(&doc, "missing"), (0, 0));
        let view = with_totals(doc);
        assert_eq!(view["totals"]["messages"], 2);
        assert_eq!(view["totals"]["attachments"], 1);
        assert!(view["totals"].get("bots").is_none());
    }

    #[test]
    fn tokens_sum_finished_turns_and_skip_what_t3_cannot_price() {
        let projection = json!({
            "providerTurns": [
                { "turnTokenUsage": { "inputTokens": 10, "outputTokens": 4, "cachedInputTokens": 3, "cacheCreationTokens": 1 } },
                { "turnTokenUsage": { "usageStatus": "unavailable", "inputTokens": 99, "outputTokens": 99 } },
                { "turnTokenUsage": { "localCommand": true, "inputTokens": 50, "outputTokens": 50 } },
                { "completedAt": "2026-11-10T00:00:00Z" }
            ]
        });
        let (totals, known) = tokens_from_projection(&projection);
        assert!(known);
        assert_eq!(totals.turns, 1);
        assert_eq!(totals.input, 10);
        assert_eq!(totals.output, 4);
        assert_eq!(totals.cached, 3);
        assert_eq!(totals.cache_write, 1);
        let (empty, seen) = tokens_from_projection(&json!({ "providerTurns": [] }));
        assert!(!seen);
        assert_eq!(empty, TokenTotals::default());
        let body = bot_payload(2, 1, &totals, true, "");
        assert_eq!(body["messages"], 2);
        assert_eq!(body["tokensKnown"], true);
        assert_eq!(body["error"], "");
    }

    #[tokio::test]
    async fn a_missing_file_does_not_invent_counts() {
        let dir = tempfile::tempdir().unwrap();
        let usage = Usage::new(dir.path().join("missing.json"));
        let snap = usage.snapshot().await.unwrap();
        assert_eq!(snap["days"], json!({}));
        assert!(snap["totals"].as_object().unwrap().is_empty());
        assert!(snap["totals"].get("messages").is_none());
    }
}
