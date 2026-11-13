//! The durable typed outbox: at most one pending write per (type, natural key).
//! A later write for the same key merges its properties into the pending item,
//! so rapid edits collapse into one remote call. A drained item is dropped on
//! remote confirmation and backs off exponentially on failure. One idempotency
//! key covers `device | type | natural key | op | content hash`, so a replay of
//! the same resolved content cannot enqueue twice at the remote.

use serde_json::{Map, Value};
use std::path::PathBuf;

const BASE_BACKOFF_MS: i64 = 2_000;
const MAX_BACKOFF_MS: i64 = 600_000;

pub(crate) struct Outbox {
    pub(crate) path: PathBuf,
    items: Vec<Item>,
}

struct Item {
    type_key: String,
    natural_key: String,
    props: Map<String, Value>,
    attempts: u32,
    next_at: i64,
}

fn backoff_ms(attempts: u32) -> i64 {
    let doubled = BASE_BACKOFF_MS.saturating_mul(1 << attempts.min(9));
    doubled.min(MAX_BACKOFF_MS)
}

fn unix_ms() -> i64 {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

impl Outbox {
    /// Load the journal. A corrupt line is skipped, keeping the rest: this file
    /// is a queue, not a document.
    pub(crate) fn load(path: PathBuf) -> Self {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let items = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|row| {
                let props = row.get("props")?.as_object()?.clone();
                let type_key = row["typeKey"].as_str()?.to_string();
                let natural_key = row["naturalKey"].as_str()?.to_string();
                Some(Item {
                    type_key,
                    natural_key,
                    props,
                    attempts: u32::try_from(row["attempts"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX),
                    next_at: row["nextAt"].as_i64().unwrap_or(0),
                })
            })
            .collect();
        Self { path, items }
    }

    /// Queue or merge one write. Returns true when this is a new entry.
    pub(crate) fn push(
        &mut self,
        type_key: &str,
        natural_key: &str,
        props: &Map<String, Value>,
    ) -> bool {
        if let Some(item) = self.find_mut(type_key, natural_key) {
            for (key, value) in props {
                item.next_at = 0;
                item.props.insert(key.clone(), value.clone());
            }
            return false;
        }
        self.items.push(Item {
            type_key: type_key.to_string(),
            natural_key: natural_key.to_string(),
            props: props.clone(),
            attempts: 0,
            next_at: 0,
        });
        true
    }

    /// The combined pending properties for one target, or `None`.
    pub(crate) fn merged(&self, type_key: &str, natural_key: &str) -> Option<Map<String, Value>> {
        self.find(type_key, natural_key).map(|item| item.props.clone())
    }

    /// Targets whose backoff window has passed, in write order.
    pub(crate) fn due(&self, now_ms: i64) -> Vec<(String, String)> {
        self.items
            .iter()
            .filter(|item| item.next_at <= now_ms)
            .map(|item| (item.type_key.clone(), item.natural_key.clone()))
            .collect()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The remote acknowledged this target's latest content.
    pub(crate) fn applied(&mut self, type_key: &str, natural_key: &str) {
        self.items
            .retain(|item| item.type_key != type_key || item.natural_key != natural_key);
    }

    /// The write failed; back off before looking again.
    pub(crate) fn failed(&mut self, type_key: &str, natural_key: &str) {
        if let Some(item) = self.find_mut(type_key, natural_key) {
            item.attempts = item.attempts.saturating_add(1);
            item.next_at = unix_ms() + backoff_ms(item.attempts);
        }
    }

    /// The whole queue as journal text; the caller writes it outside the lock.
    pub(crate) fn journal_text(&self) -> String {
        let mut text = String::new();
        for item in &self.items {
            let row = serde_json::json!({
                "typeKey": item.type_key,
                "naturalKey": item.natural_key,
                "props": Value::Object(item.props.clone()),
                "attempts": item.attempts,
                "nextAt": item.next_at,
            });
            text.push_str(&row.to_string());
            text.push('\n');
        }
        text
    }

    fn find(&self, type_key: &str, natural_key: &str) -> Option<&Item> {
        self.items
            .iter()
            .find(|item| item.type_key == type_key && item.natural_key == natural_key)
    }

    fn find_mut(&mut self, type_key: &str, natural_key: &str) -> Option<&mut Item> {
        self.items
            .iter_mut()
            .find(|item| item.type_key == type_key && item.natural_key == natural_key)
    }
}

#[cfg(test)]
impl Outbox {
    pub(crate) async fn persist_journal(&self) -> std::io::Result<()> {
        let text = self.journal_text();
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        crate::storage::write_atomic(&self.path, text.as_bytes(), None).await
    }
}
