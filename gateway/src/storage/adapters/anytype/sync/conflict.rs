//! An open conflict keeps the local file, writes the remote bytes beside it, and
//! records both copies on an `m_conflict` object. The path stays frozen until resolved.

use super::super::client::{AnytypeClient, AnytypeError};
use super::error::SyncError;
use super::outbox::{self, Op};
use super::payload::{self, wire_key};
use super::sha256_hex;
use super::unix_ms;
use crate::storage::write_atomic;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ConflictState {
    Open,
    Resolved,
    Dismissed,
}

impl ConflictState {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
            Self::Dismissed => "dismissed",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "resolved" => Self::Resolved,
            "dismissed" => Self::Dismissed,
            _ => Self::Open,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Record {
    pub id: String,
    pub path: String,
    pub object_id: Option<String>,
    pub local_sha: String,
    pub remote_sha: String,
    pub sidecar: String,
    pub state: ConflictState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct File {
    version: u32,
    items: Vec<Record>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Conflicts {
    items: Vec<Record>,
}

impl Conflicts {
    pub(super) fn open_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.state == ConflictState::Open)
            .count()
    }

    pub(super) fn note_object(&mut self, path: &str, object_id: &str) -> bool {
        let Some(existing) = self
            .items
            .iter_mut()
            .find(|item| item.path == path && item.state == ConflictState::Open)
        else {
            return false;
        };
        if existing.object_id.as_deref() == Some(object_id) {
            return false;
        }
        existing.object_id = Some(object_id.to_string());
        true
    }

    pub(super) fn open_for(&self, path: &str) -> bool {
        self.items
            .iter()
            .any(|item| item.path == path && item.state == ConflictState::Open)
    }

    pub(super) fn open_items(&self) -> impl Iterator<Item = &Record> {
        self.items
            .iter()
            .filter(|item| item.state == ConflictState::Open)
    }

    pub(super) fn by_id(&self, id: &str) -> Option<&Record> {
        self.items.iter().find(|item| item.id == id)
    }

    pub(super) fn insert(&mut self, record: Record) {
        if let Some(existing) = self
            .items
            .iter_mut()
            .find(|item| item.path == record.path && item.state == ConflictState::Open)
        {
            *existing = record;
            return;
        }
        self.items.push(record);
    }

    pub(super) fn remove_path(&mut self, path: &str) -> Option<Record> {
        let index = self.items.iter().position(|item| item.path == path)?;
        Some(self.items.remove(index))
    }

    pub(super) fn remove_id(&mut self, id: &str) -> Option<Record> {
        let index = self.items.iter().position(|item| item.id == id)?;
        Some(self.items.remove(index))
    }

    pub(super) fn load(path: &Path) -> Result<Self, SyncError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(SyncError::Io(error)),
        };
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        let file: File = serde_json::from_str(&text)
            .map_err(|error| SyncError::Conflict(format!("conflicts.json: {error}")))?;
        Ok(Self { items: file.items })
    }

    pub(super) async fn save(&self, path: &Path) -> Result<(), SyncError> {
        let file = File {
            version: 1,
            items: self.items.clone(),
        };
        let text = serde_json::to_string_pretty(&file).expect("conflicts are json");
        write_atomic(path, format!("{text}\n").as_bytes(), Some(0o600))
            .await
            .map_err(SyncError::Io)
    }
}

pub(super) fn sidecar_rel(rel: &str, device_id: &str, unix_secs: u64) -> String {
    let device: String = device_id.chars().take(8).collect();
    let stamp = utc_compact(unix_secs);
    let (dir, name) = match rel.rsplit_once('/') {
        Some((dir, name)) => (dir, name),
        None => ("", rel),
    };
    let file = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => {
            format!("{stem}.conflict-{device}-{stamp}.{ext}")
        }
        Some(_) | None => format!("{name}.conflict-{device}-{stamp}"),
    };
    if dir.is_empty() {
        file
    } else {
        format!("{dir}/{file}")
    }
}

pub(super) fn sibling_rel(rel: &str, unix_secs: u64) -> String {
    let (dir, name) = match rel.rsplit_once('/') {
        Some((dir, name)) => (dir, name),
        None => ("", rel),
    };
    let file = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => {
            format!("{stem}-from-anytype-{unix_secs}.{ext}")
        }
        Some(_) | None => format!("{name}-from-anytype-{unix_secs}"),
    };
    if dir.is_empty() {
        file
    } else {
        format!("{dir}/{file}")
    }
}

pub(super) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) async fn publish(
    client: &AnytypeClient,
    device_id: &str,
    rel: &str,
    local_sha: &str,
    remote_sha: &str,
    local: &[u8],
    remote: &[u8],
) -> Result<String, AnytypeError> {
    if let Some(existing) = find_open(client, rel).await? {
        return Ok(existing);
    }
    let copy_a = upload_copy(client, device_id, rel, "a", local_sha, local).await?;
    let copy_b = upload_copy(client, device_id, rel, "b", remote_sha, remote).await?;
    let properties = json!({
        "m_path": rel,
        "m_owner_bot": payload::owner(rel),
        "m_conflict_state": "open",
        "m_device_a": device_id,
        "m_device_b": "",
        "m_sha_a": local_sha,
        "m_sha_b": remote_sha,
        "m_copy_a": [copy_a],
        "m_copy_b": [copy_b],
        "m_detected_at": unix_ms(),
    });
    let body = json!({
        "type": "m_conflict",
        "name": rel,
        "properties": properties,
    });
    let key = outbox::idem_key(device_id, &format!("conflict:{rel}"), Op::Create, local_sha);
    let created = client.create_object(&body, &wire_key(&key, None)).await?;
    payload::id_of(&created).ok_or(AnytypeError::ApiChanged)
}

pub(super) async fn mark(
    client: &AnytypeClient,
    device_id: &str,
    rel: &str,
    object_id: &str,
    state: ConflictState,
) -> Result<(), AnytypeError> {
    let body = json!({
        "ops": [{
            "op": "set_properties",
            "set": {"m_conflict_state": state.as_str()},
        }],
    });
    let key = outbox::idem_key(
        device_id,
        &format!("conflict:{rel}"),
        Op::Update,
        state.as_str(),
    );
    client
        .patch_object(object_id, &body, None, &wire_key(&key, None))
        .await?;
    Ok(())
}

pub(super) fn record_for(
    rel: &str,
    object_id: Option<String>,
    local_sha: &str,
    remote_sha: &str,
    sidecar: &str,
) -> Record {
    Record {
        id: uuid::Uuid::new_v4().to_string(),
        path: rel.to_string(),
        object_id,
        local_sha: local_sha.to_string(),
        remote_sha: remote_sha.to_string(),
        sidecar: sidecar.to_string(),
        state: ConflictState::Open,
    }
}

pub(super) fn conflicts_path(sync_dir: &Path) -> PathBuf {
    sync_dir.join("conflicts.json")
}

pub(super) fn state_of(text: &str) -> ConflictState {
    ConflictState::parse(text)
}

async fn find_open(client: &AnytypeClient, rel: &str) -> Result<Option<String>, AnytypeError> {
    let rows = client
        .search(&json!({
            "type": "m_conflict",
            "fields": ["m_path", "m_conflict_state"],
        }))
        .await?;
    for row in rows {
        let path = row
            .get("properties")
            .and_then(|props| props.get("m_path"))
            .and_then(Value::as_str);
        let state = row
            .get("properties")
            .and_then(|props| props.get("m_conflict_state"))
            .and_then(Value::as_str)
            .unwrap_or("open");
        if path == Some(rel) && state == "open" {
            return Ok(row.get("id").and_then(Value::as_str).map(str::to_string));
        }
    }
    Ok(None)
}

async fn upload_copy(
    client: &AnytypeClient,
    device: &str,
    rel: &str,
    side: &str,
    sha: &str,
    bytes: &[u8],
) -> Result<String, AnytypeError> {
    let key = outbox::idem_key(device, &format!("conflict-{side}:{rel}"), Op::Create, sha);
    let name = format!("{side}-{}.bin", sha256_hex(rel.as_bytes()));
    let uploaded = client
        .upload_bytes(
            &name,
            "application/octet-stream",
            bytes,
            &wire_key(&key, None),
        )
        .await?;
    payload::id_of(&uploaded).ok_or(AnytypeError::ApiChanged)
}

fn utc_compact(unix: u64) -> String {
    let days = i64::try_from(unix / 86_400).unwrap_or(0);
    let tod = unix % 86_400;
    let hour = tod / 3600;
    let minute = (tod % 3600) / 60;
    let second = tod % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

/// Howard Hinnant's `civil_from_days`, days since the Unix epoch.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = u64::try_from(z - era * 146_097).unwrap_or(0);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year_of_era = i64::try_from(yoe).unwrap_or(0);
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 {
        year_of_era + era * 400 + 1
    } else {
        year_of_era + era * 400
    };
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}
