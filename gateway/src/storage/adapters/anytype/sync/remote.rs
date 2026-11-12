//! Remote listing. Cheap fields only. Content is fetched when those fields say it changed.
//! Two objects with one `hb_path` keep the earliest `created_date`. The others are duplicates.

use super::super::client::{AnytypeClient, AnytypeError};
use super::manifest::Entry;
use super::policy::{classify, Class};
use super::sha256_hex;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RemoteFile {
    pub object_id: String,
    pub advisory_sha: String,
    pub deleted: bool,
    pub modified: Option<String>,
    pub created: String,
    pub device: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Duplicate {
    pub path: String,
    pub object_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RemoteConflict {
    pub object_id: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Listing {
    pub files: BTreeMap<String, RemoteFile>,
    pub duplicates: Vec<Duplicate>,
    pub conflicts: BTreeMap<String, RemoteConflict>,
    pub skipped: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Loaded {
    pub bytes: Vec<u8>,
    pub sha: String,
    pub etag: Option<String>,
    pub deleted: bool,
    pub modified: Option<String>,
}

impl Listing {
    pub(super) fn contains_id(&self, id: &str) -> bool {
        self.files.values().any(|file| file.object_id == id)
            || self.duplicates.iter().any(|dup| dup.object_id == id)
    }
}

pub(super) async fn list(client: &AnytypeClient) -> Result<Listing, AnytypeError> {
    let files = client
        .search(&json!({
            "type": "hb_file",
            "fields": [
                "hb_path", "hb_sha", "hb_deleted", "hb_device", "hb_rev", "hb_size",
                "created_date", "last_modified_date",
            ],
        }))
        .await?;
    let conflicts = client
        .search(&json!({
            "type": "hb_conflict",
            "fields": ["hb_path", "hb_conflict_state"],
        }))
        .await?;
    Ok(parse(&files, &conflicts))
}

pub(super) async fn load(client: &AnytypeClient, id: &str) -> Result<Loaded, AnytypeError> {
    let value = client.get_object(id).await?;
    parse_object(client, &value).await
}

pub(super) fn needs_body(entry: Option<&Entry>, remote: &RemoteFile) -> bool {
    if remote.deleted {
        return false;
    }
    if remote.advisory_sha.is_empty() {
        return true;
    }
    let Some(entry) = entry else {
        return true;
    };
    if entry.base_sha.as_deref() != Some(remote.advisory_sha.as_str()) {
        return true;
    }
    match (&entry.remote_modified, &remote.modified) {
        (Some(prev), Some(next)) => next > prev,
        (None, Some(_)) => true,
        (Some(_), None) | (None, None) => false,
    }
}

fn parse(files: &[Value], conflicts: &[Value]) -> Listing {
    let mut skipped = 0_u64;
    let mut grouped: BTreeMap<String, Vec<RemoteFile>> = BTreeMap::new();
    for row in files {
        match parse_file(row) {
            Some((path, file)) => grouped.entry(path).or_default().push(file),
            None => skipped += 1,
        }
    }
    let mut chosen = BTreeMap::new();
    let mut duplicates = Vec::new();
    for (path, mut group) in grouped {
        group.sort_by(|left, right| {
            left.created
                .cmp(&right.created)
                .then_with(|| id_number(&left.object_id).cmp(&id_number(&right.object_id)))
        });
        let mut group = group.into_iter();
        if let Some(winner) = group.next() {
            chosen.insert(path.clone(), winner);
        }
        for extra in group {
            duplicates.push(Duplicate {
                path: path.clone(),
                object_id: extra.object_id,
            });
        }
    }
    let mut conflict_map: BTreeMap<String, RemoteConflict> = BTreeMap::new();
    for row in conflicts {
        let Some(path) = text_at(row, "hb_path") else {
            skipped += 1;
            continue;
        };
        if !matches!(classify(&path), Class::Allow { .. }) {
            skipped += 1;
            continue;
        }
        let Some(object_id) = row.get("id").and_then(Value::as_str) else {
            skipped += 1;
            continue;
        };
        let state = text_at(row, "hb_conflict_state").unwrap_or_else(|| "open".to_string());
        let next = RemoteConflict {
            object_id: object_id.to_string(),
            state,
        };
        let keep_open = conflict_map
            .get(&path)
            .is_some_and(|current| current.state == "open");
        if !keep_open {
            conflict_map.insert(path, next);
        }
    }
    Listing {
        files: chosen,
        duplicates,
        conflicts: conflict_map,
        skipped,
    }
}

fn parse_file(row: &Value) -> Option<(String, RemoteFile)> {
    let path = text_at(row, "hb_path")?;
    if !matches!(classify(&path), Class::Allow { .. }) {
        return None;
    }
    let object_id = row.get("id").and_then(Value::as_str)?.to_string();
    let created = text_at(row, "created_date").unwrap_or_default();
    let modified = text_at(row, "last_modified_date");
    Some((
        path,
        RemoteFile {
            object_id,
            advisory_sha: text_at(row, "hb_sha").unwrap_or_default(),
            deleted: bool_at(row, "hb_deleted"),
            modified,
            created,
            device: text_at(row, "hb_device"),
        },
    ))
}

async fn parse_object(client: &AnytypeClient, value: &Value) -> Result<Loaded, AnytypeError> {
    let props = value.get("properties");
    let deleted = props
        .and_then(|props| props.get("hb_deleted"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let modified = text_at(value, "last_modified_date");
    let etag = value
        .get("etag")
        .and_then(Value::as_str)
        .filter(|etag| !etag.is_empty())
        .map(str::to_string);
    let blob = props.and_then(first_blob);
    let bytes = if let Some(file_id) = blob {
        client.download_file(&file_id).await?
    } else if let Some(text) = props
        .and_then(|props| props.get("hb_content"))
        .and_then(Value::as_str)
    {
        text.as_bytes().to_vec()
    } else {
        Vec::new()
    };
    let sha = sha256_hex(&bytes);
    Ok(Loaded {
        bytes,
        sha,
        etag,
        deleted,
        modified,
    })
}

fn first_blob(props: &Value) -> Option<String> {
    let id = props.get("hb_blob")?.as_array()?.first()?.as_str()?;
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

fn text_at(row: &Value, key: &str) -> Option<String> {
    // The app nests search fields under `properties`; the mock and a few object
    // bodies keep system dates at the top level. Both shapes are read.
    row.get("properties")
        .and_then(|props| props.get(key))
        .or_else(|| row.get(key))
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn bool_at(row: &Value, key: &str) -> bool {
    row.get("properties")
        .and_then(|props| props.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn id_number(id: &str) -> u64 {
    id.rsplit(|ch: char| !ch.is_ascii_digit())
        .next()
        .and_then(|text| text.parse().ok())
        .unwrap_or(0)
}
