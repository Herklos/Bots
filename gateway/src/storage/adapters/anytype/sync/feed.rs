//! Live `hb_file` search stream. Events are hints: the engine lists and fetches the
//! path. A dropped or refused stream (cap, 429) does not stop the loop; the caller
//! polls. A reconnect is a new snapshot, diffed against the manifest. An id in the
//! manifest but missing from that snapshot is only a hint — the cycle reads the
//! object before it treats the path as gone.

use super::super::client::{AnytypeClient, AnytypeError, StreamEvent};
use super::engine::Engine;
use super::manifest::Manifest;
use super::status;
use serde_json::Value;
use std::collections::BTreeSet;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy)]
pub(super) struct FeedSettings {
    pub heartbeat: Duration,
    pub idle: Duration,
    pub backoff_floor: Duration,
}

impl FeedSettings {
    pub(super) fn production() -> Self {
        Self {
            heartbeat: Duration::from_secs(15),
            idle: Duration::from_secs(45),
            backoff_floor: Duration::from_secs(2),
        }
    }
}

#[derive(Debug)]
pub(super) enum Note {
    Up,
    /// `refused` is a 429 (stream cap or a rate limit). Anything else is the stream
    /// ending or the host being down.
    Down {
        refused: bool,
    },
    Snapshot(Vec<Value>),
    Changed(Value),
    Removed(String),
}

pub(super) fn spawn(
    client: AnytypeClient,
    settings: FeedSettings,
    notes: mpsc::UnboundedSender<Note>,
) {
    tokio::spawn(async move {
        run(client, settings, notes).await;
    });
}

async fn run(client: AnytypeClient, settings: FeedSettings, notes: mpsc::UnboundedSender<Note>) {
    let mut backoff = settings.backoff_floor;
    loop {
        if notes.is_closed() {
            return;
        }
        match client.open_file_stream(settings.heartbeat).await {
            Ok(stream) => {
                let saw = pump(&notes, stream, settings.idle).await;
                if notes.is_closed() {
                    return;
                }
                let _ = notes.send(Note::Down { refused: false });
                if !pause(&notes, backoff).await {
                    return;
                }
                backoff = if saw {
                    settings.backoff_floor
                } else {
                    grow(backoff)
                };
            }
            Err(AnytypeError::RateLimited { retry_after }) => {
                if notes.send(Note::Down { refused: true }).is_err() {
                    return;
                }
                let wait = retry_after.max(backoff);
                if !pause(&notes, wait).await {
                    return;
                }
                backoff = grow(wait);
            }
            Err(error) => {
                tracing::debug!("Anytype search stream is down: {error:?}");
                if notes.send(Note::Down { refused: false }).is_err() {
                    return;
                }
                if !pause(&notes, backoff).await {
                    return;
                }
                backoff = grow(backoff);
            }
        }
    }
}

/// Reads until the stream ends or goes quiet. True once `snapshot_complete` arrived.
async fn pump(
    notes: &mpsc::UnboundedSender<Note>,
    mut stream: super::super::client::FileStream,
    idle: Duration,
) -> bool {
    let mut batch = Vec::new();
    let mut live = false;
    loop {
        let event = tokio::select! {
            _ = notes.closed() => return live,
            next = stream.next_event() => next,
            _ = tokio::time::sleep(idle) => return live,
        };
        match event {
            Ok(None) | Err(_) => return live,
            Ok(Some(StreamEvent::Heartbeat)) => {}
            Ok(Some(StreamEvent::SnapshotComplete)) => {
                live = true;
                if notes
                    .send(Note::Snapshot(std::mem::take(&mut batch)))
                    .is_err()
                {
                    return true;
                }
                if notes.send(Note::Up).is_err() {
                    return true;
                }
            }
            Ok(Some(StreamEvent::Added(row))) if !live => batch.push(row),
            Ok(Some(StreamEvent::Added(row) | StreamEvent::Updated(row))) => {
                if notes.send(Note::Changed(row)).is_err() {
                    return live;
                }
            }
            Ok(Some(StreamEvent::Removed { id })) => {
                if notes.send(Note::Removed(id)).is_err() {
                    return live;
                }
            }
        }
    }
}

async fn pause(notes: &mpsc::UnboundedSender<Note>, wait: Duration) -> bool {
    tokio::select! {
        _ = notes.closed() => false,
        _ = tokio::time::sleep(wait) => true,
    }
}

fn grow(current: Duration) -> Duration {
    let doubled = current.saturating_mul(2);
    if doubled > Duration::from_secs(60) {
        Duration::from_secs(60)
    } else if doubled < current {
        current
    } else {
        doubled
    }
}

/// Apply one stream note. True when the engine should list and fetch now.
pub(super) fn ingest(engine: &mut Engine, note: Note, stream_up: &mut bool) -> bool {
    match note {
        Note::Up => {
            *stream_up = true;
            false
        }
        Note::Down { .. } => {
            *stream_up = false;
            true
        }
        Note::Snapshot(rows) => {
            let (changed, missing) =
                diff_snapshot(&engine.manifest, &engine.device_id, &engine.echo, &rows);
            for path in changed.iter().chain(&missing) {
                engine.nudge(path);
            }
            !changed.is_empty() || !missing.is_empty()
        }
        Note::Changed(row) => {
            if is_echo(engine, &row) {
                return false;
            }
            if let Some(path) = field(&row, "hb_path") {
                engine.nudge(&path);
            }
            true
        }
        Note::Removed(id) => {
            if let Some(path) = path_of(&engine.manifest, &id) {
                engine.nudge(&path);
            }
            true
        }
    }
}

fn diff_snapshot(
    manifest: &Manifest,
    device: &str,
    echo: &super::echo::EchoBook,
    rows: &[Value],
) -> (Vec<String>, Vec<String>) {
    let mut seen = BTreeSet::new();
    let mut changed = Vec::new();
    for row in rows {
        let Some(id) = row.get("id").and_then(Value::as_str) else {
            continue;
        };
        seen.insert(id.to_string());
        let Some(path) = field(row, "hb_path") else {
            continue;
        };
        if echo_of(device, echo, &path, row) {
            continue;
        }
        if path_changed(manifest, &path, row) {
            changed.push(path);
        }
    }
    let missing = manifest
        .entries
        .iter()
        .filter(|(_, entry)| {
            entry
                .object_id
                .as_deref()
                .is_some_and(|id| !seen.contains(id))
        })
        .map(|(path, _)| path.clone())
        .collect();
    (changed, missing)
}

fn path_changed(manifest: &Manifest, path: &str, row: &Value) -> bool {
    let Some(entry) = manifest.entries.get(path) else {
        return true;
    };
    let deleted = flag(row, "hb_deleted");
    if deleted != entry.tombstone {
        return true;
    }
    let sha = field(row, "hb_sha").unwrap_or_default();
    if sha.is_empty() || entry.base_sha.as_deref() != Some(sha.as_str()) {
        return true;
    }
    let id = row.get("id").and_then(Value::as_str);
    entry.object_id.as_deref() != id
}

fn is_echo(engine: &Engine, row: &Value) -> bool {
    let Some(path) = field(row, "hb_path") else {
        return false;
    };
    echo_of(&engine.device_id, &engine.echo, &path, row)
}

fn echo_of(device: &str, echo: &super::echo::EchoBook, path: &str, row: &Value) -> bool {
    let Some(sha) = field(row, "hb_sha") else {
        return false;
    };
    field(row, "hb_device").as_deref() == Some(device) && echo.matches(path, &sha)
}

fn path_of(manifest: &Manifest, id: &str) -> Option<String> {
    manifest
        .entries
        .iter()
        .find(|(_, entry)| entry.object_id.as_deref() == Some(id))
        .map(|(path, _)| path.clone())
}

fn field(row: &Value, key: &str) -> Option<String> {
    row.get("properties")
        .and_then(|props| props.get(key))
        .or_else(|| row.get(key))
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn flag(row: &Value, key: &str) -> bool {
    row.get("properties")
        .and_then(|props| props.get(key))
        .or_else(|| row.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// The 10s poll is the ceiling when the stream is refused or down and Anytype
/// still answers. A scan that already runs sooner stays on that shorter timer.
pub(super) fn cycle_wait(offline: bool, backoff: Duration, stream_up: bool) -> Duration {
    let scan = status::next_wait(offline, backoff, false);
    if stream_up || offline {
        scan
    } else {
        scan.min(Duration::from_secs(10))
    }
}
