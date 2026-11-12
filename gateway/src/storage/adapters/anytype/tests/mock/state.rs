//! In-memory Anytype records and the fault switches.

use super::objects::Block;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

/// A mutation the search-stream subscribers should notice. `gen` orders it against a snapshot.
#[derive(Clone)]
pub(super) enum Live {
    Changed { id: String, gen: u64 },
    DropAll,
}

#[derive(Clone)]
pub(super) struct Object {
    pub id: String,
    pub name: String,
    pub type_key: String,
    pub properties: Value,
    pub markdown: String,
    pub blocks: Vec<Block>,
    pub etag: String,
    pub archived: bool,
    pub created_date: String,
    pub last_modified: Option<String>,
}

#[derive(Clone)]
pub(super) struct FileBlob {
    pub bytes: Vec<u8>,
    pub sha: String,
}

#[derive(Clone)]
struct Replay {
    fingerprint: String,
    status: u16,
    body: Value,
}

pub(super) struct Reply {
    pub status: u16,
    pub body: Value,
    pub etag: Option<String>,
}

pub(super) struct MockState {
    pub objects: BTreeMap<String, Object>,
    pub files: BTreeMap<String, FileBlob>,
    pub types: Vec<Value>,
    pub properties: Vec<Value>,
    next: u64,
    etag_clock: u64,
    tick: u64,
    replays: BTreeMap<String, Replay>,
    pub not_granted: bool,
    pub malformed: bool,
    pub etag_mismatch: bool,
    pub archived_writes: bool,
    pub status_override: Option<u16>,
    pub rate_limits_left: u32,
    pub retry_after_secs: u64,
    pub delay: Duration,
    pub rewrites: BTreeMap<String, String>,
    pub stream_cap: usize,
    pub stream_open: usize,
    pub stream_omit: BTreeSet<String>,
    pub gets: u64,
    pub last_stream_body: Value,
    pub gen: u64,
    pub live: tokio::sync::broadcast::Sender<Live>,
}

impl MockState {
    pub(super) fn new() -> Self {
        Self {
            objects: BTreeMap::new(),
            files: BTreeMap::new(),
            types: Vec::new(),
            properties: Vec::new(),
            next: 1,
            etag_clock: 0,
            tick: 0,
            replays: BTreeMap::new(),
            not_granted: false,
            malformed: false,
            etag_mismatch: false,
            archived_writes: false,
            status_override: None,
            rate_limits_left: 0,
            retry_after_secs: 0,
            delay: Duration::ZERO,
            rewrites: BTreeMap::new(),
            stream_cap: 8,
            stream_open: 0,
            stream_omit: BTreeSet::new(),
            gets: 0,
            last_stream_body: Value::Null,
            gen: 0,
            live: tokio::sync::broadcast::channel(64).0,
        }
    }

    pub(super) fn announce(&mut self, id: &str) {
        self.gen = self.gen.saturating_add(1);
        let _ = self.live.send(Live::Changed {
            id: id.to_string(),
            gen: self.gen,
        });
    }

    pub(super) fn drop_streams(&mut self) {
        let _ = self.live.send(Live::DropAll);
    }

    pub(super) fn seed_objects(&mut self, count: usize) {
        for index in 0..count {
            let body = serde_json::json!({
                "name": format!("note-{index}"),
                "type": "page",
            });
            self.create_object(&body);
        }
    }

    pub(super) fn seed_types(&mut self, count: usize) {
        for index in 0..count {
            let body = serde_json::json!({
                "name": format!("Type {index}"),
                "api_key": format!("type_{index}"),
                "layout": "basic",
            });
            self.add_type(&body);
        }
    }

    pub(super) fn alloc(&mut self, prefix: &str) -> String {
        let id = format!("{prefix}-{}", self.next);
        self.next += 1;
        id
    }

    pub(super) fn next_etag(&mut self) -> String {
        self.etag_clock += 1;
        format!("{:08x}", self.etag_clock)
    }

    pub(super) fn now(&self) -> String {
        stamp(self.tick)
    }

    pub(super) fn touch(&mut self) -> String {
        self.tick += 1;
        stamp(self.tick)
    }

    /// Digits split a snake key (`hb_probe2` comes back `hb_probe_2`).
    /// An explicit rewrite, used by the drift test, wins over that rule.
    pub(super) fn finish_key(&self, key: &str) -> String {
        if let Some(rewritten) = self.rewrites.get(key) {
            return rewritten.clone();
        }
        let normalized = normalize_key(key);
        self.rewrites
            .get(&normalized)
            .cloned()
            .unwrap_or(normalized)
    }

    pub(super) fn replay(&self, key: &str, fingerprint: &str) -> ReplayHit {
        match self.replays.get(key) {
            None => ReplayHit::Miss,
            Some(prior) if prior.fingerprint == fingerprint => {
                ReplayHit::Hit(prior.status, prior.body.clone())
            }
            Some(_) => ReplayHit::Conflict,
        }
    }

    pub(super) fn store_replay(
        &mut self,
        key: &str,
        fingerprint: String,
        status: u16,
        body: Value,
    ) {
        self.replays.insert(
            key.to_string(),
            Replay {
                fingerprint,
                status,
                body,
            },
        );
    }
}

pub(super) fn normalize_key(key: &str) -> String {
    let mut out = String::new();
    for ch in key.chars() {
        if ch.is_ascii_digit() && !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
        out.push(ch);
    }
    out
}

pub(super) fn snake_name(name: &str) -> String {
    name.split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("_")
}

pub(super) fn sha_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub(super) enum ReplayHit {
    Miss,
    Hit(u16, Value),
    Conflict,
}

pub(super) fn page(rows: &[Value], offset: usize, limit: usize) -> Value {
    let total = rows.len();
    let end = (offset + limit).min(total);
    let slice = if offset >= total {
        &[]
    } else {
        &rows[offset..end]
    };
    serde_json::json!({
        "data": slice,
        "total": total,
        "offset": offset,
        "limit": limit,
        "has_more": end < total,
    })
}

fn stamp(step: u64) -> String {
    let total = 16 * 3600 + 52 * 60 + 18 + step;
    let hour = (total / 3600) % 24;
    let minute = (total % 3600) / 60;
    let second = total % 60;
    let day = 12 + total / 86_400;
    format!("2026-11-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}
