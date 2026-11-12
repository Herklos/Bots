//! Pending remote writes. A replay of the same idempotency key does not enqueue twice.
//! The key is `sha256(device_id | path | op | sha)` with `|` between the fields.
//! `next_at` is unix milliseconds.

use super::sha256_hex;
use crate::storage::write_atomic;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;

pub(super) const WARN_AT: usize = 1_000;
pub(super) const REFUSE_AT: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Op {
    Create,
    Update,
    Tombstone,
}

impl Op {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Tombstone => "tombstone",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Item {
    pub id: String,
    pub path: String,
    pub op: Op,
    pub sha: String,
    pub idem_key: String,
    pub attempts: u32,
    pub next_at: i64,
}

impl Item {
    pub(super) fn new(
        id: impl Into<String>,
        path: &str,
        op: Op,
        sha: &str,
        device_id: &str,
    ) -> Self {
        let path = path.to_string();
        let sha = sha.to_string();
        let idem_key = idem_key(device_id, &path, op, &sha);
        Self {
            id: id.into(),
            path,
            op,
            sha,
            idem_key,
            attempts: 0,
            next_at: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Push {
    Queued { warn: bool },
    Replay,
}

#[derive(Debug)]
pub(super) enum OutboxError {
    Full,
    Corrupt(String),
    Io(io::Error),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Outbox {
    items: Vec<Item>,
}

impl Outbox {
    pub(super) fn items(&self) -> &[Item] {
        &self.items
    }

    pub(super) fn push(&mut self, item: Item) -> Result<Push, OutboxError> {
        if self
            .items
            .iter()
            .any(|existing| existing.idem_key == item.idem_key)
        {
            return Ok(Push::Replay);
        }
        if self.items.len() >= REFUSE_AT {
            return Err(OutboxError::Full);
        }
        let warn = self.items.len() + 1 >= WARN_AT;
        self.items.push(item);
        Ok(Push::Queued { warn })
    }

    pub(super) fn ack(&mut self, id: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|item| item.id != id);
        self.items.len() != before
    }

    /// Drops every pending write for `path`. A finished path must not replay an older body.
    pub(super) fn ack_path(&mut self, path: &str) -> usize {
        let before = self.items.len();
        self.items.retain(|item| item.path != path);
        before - self.items.len()
    }

    pub(super) fn defer(&mut self, id: &str, next_at: i64) {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.attempts = item.attempts.saturating_add(1);
            item.next_at = next_at;
        }
    }

    pub(super) fn id_for_key(&self, key: &str) -> Option<String> {
        self.items
            .iter()
            .find(|item| item.idem_key == key)
            .map(|item| item.id.clone())
    }

    pub(super) async fn save(&self, path: &Path) -> Result<(), OutboxError> {
        let mut body = String::new();
        for item in &self.items {
            let line = serde_json::to_string(item).expect("outbox item is json");
            body.push_str(&line);
            body.push('\n');
        }
        write_atomic(path, body.as_bytes(), Some(0o600))
            .await
            .map_err(OutboxError::Io)
    }

    pub(super) fn load(path: &Path) -> Result<Self, OutboxError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(OutboxError::Io(error)),
        };
        let mut items = Vec::new();
        for (index, line) in text.lines().enumerate() {
            if line.is_empty() {
                continue;
            }
            let item: Item = serde_json::from_str(line)
                .map_err(|err| OutboxError::Corrupt(format!("line {}: {err}", index + 1)))?;
            if items
                .iter()
                .any(|existing: &Item| existing.idem_key == item.idem_key)
            {
                continue;
            }
            items.push(item);
        }
        Ok(Self { items })
    }
}

pub(super) fn idem_key(device_id: &str, path: &str, op: Op, sha: &str) -> String {
    sha256_hex(format!("{device_id}|{path}|{}|{sha}", op.name()).as_bytes())
}
