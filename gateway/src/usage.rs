//! Local usage counts in `bots/.botapp/usage.json`.
//!
//! The gateway counts what it did (messages, attachments, vault writes, icon fetches) and
//! keeps that on disk. Nothing is sent anywhere. A remote telemetry destination would come
//! from the Node server checkout, which is not on this machine.

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
        if let Err(err) = self.note_at(event, unix_now()).await {
            tracing::warn!("usage: {}", err.message);
        }
    }

    async fn note_at(&self, event: Event, unix_secs: u64) -> ApiResult<()> {
        let _g = self.lock.lock().await;
        let mut doc = read_doc(&self.path).await?;
        record(&mut doc, &day_key(unix_secs), event.key(), KEEP_DAYS);
        crate::store::write_json(&self.path, &doc).await?;
        Ok(())
    }

    pub async fn snapshot(&self) -> ApiResult<Value> {
        let _g = self.lock.lock().await;
        let doc = read_doc(&self.path).await?;
        Ok(with_totals(doc))
    }
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
                    let n = totals.get(k).and_then(Value::as_u64).unwrap_or(0);
                    totals.insert(k.clone(), json!(n + v.as_u64().unwrap_or(0)));
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
        usage.note_at(Event::Message, 1_794_268_800).await.unwrap();
        usage
            .note_at(Event::IconFetch, 1_794_268_800)
            .await
            .unwrap();
        let snap = usage.snapshot().await.unwrap();
        assert_eq!(snap["days"]["2026-11-10"]["messages"], 1);
        assert_eq!(snap["totals"]["iconFetches"], 1);
        tokio::fs::write(&path, "{").await.unwrap();
        assert!(usage.note_at(Event::Message, 0).await.is_err());
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), "{");
    }
}
