//! `.state.json` beside a memory file. A missing or damaged file is an empty state.
use super::facts::sha256_hex;
use super::MemoryStore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Provenance {
    pub author: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    pub at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Reflection {
    pub last_position: Option<i64>,
    pub last_message_id: Option<String>,
    pub at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Temporal {
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct MemoryState {
    pub version: u32,
    pub revisions: BTreeMap<String, String>,
    pub provenance: BTreeMap<String, Provenance>,
    pub forgotten: Vec<String>,
    pub reflection: Reflection,
    pub temporal: BTreeMap<String, Temporal>,
}

impl MemoryState {
    pub(super) fn empty() -> Self {
        Self {
            version: 1,
            revisions: BTreeMap::new(),
            provenance: BTreeMap::new(),
            forgotten: Vec::new(),
            reflection: Reflection::default(),
            temporal: BTreeMap::new(),
        }
    }
}

pub(crate) fn revision_of(content: &str) -> String {
    sha256_hex(&super::evidence::normalize_newlines(content))
}

pub(super) fn parse_state(raw: &str) -> MemoryState {
    if raw.trim().is_empty() {
        return MemoryState::empty();
    }
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return MemoryState::empty();
    };
    if !value.is_object() {
        return MemoryState::empty();
    }
    MemoryState {
        version: 1,
        revisions: string_map(&value["revisions"]),
        provenance: provenance_map(&value["provenance"]),
        forgotten: string_list(&value["forgotten"]),
        reflection: reflection_of(&value["reflection"]),
        temporal: temporal_map(&value["temporal"]),
    }
}

pub(super) async fn read_state(store: &MemoryStore, rel: &str) -> io::Result<MemoryState> {
    match store.read_text(rel).await {
        Ok(text) => Ok(parse_state(&text)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(MemoryState::empty()),
        Err(err) => Err(err),
    }
}

pub(super) async fn write_state(
    store: &MemoryStore,
    rel: &str,
    state: &MemoryState,
) -> io::Result<()> {
    let mut text = serde_json::to_string_pretty(state)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    text.push('\n');
    store.write_text(rel, &text, Some(0o600)).await
}

pub(super) fn tombstone(forgotten: &mut Vec<String>, key: &str) {
    if !forgotten.iter().any(|item| item == key) {
        forgotten.push(key.to_string());
    }
}

pub(super) fn clear_tombstone(forgotten: &mut Vec<String>, key: &str) {
    forgotten.retain(|item| item != key);
}

fn string_map(value: &Value) -> BTreeMap<String, String> {
    let Some(object) = value.as_object() else {
        return BTreeMap::new();
    };
    object
        .iter()
        .filter_map(|(key, item)| item.as_str().map(|text| (key.clone(), text.to_string())))
        .collect()
}

fn string_list(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn provenance_map(value: &Value) -> BTreeMap<String, Provenance> {
    let Some(object) = value.as_object() else {
        return BTreeMap::new();
    };
    object
        .iter()
        .filter_map(|(key, item)| {
            let author = item["author"].as_str()?.to_string();
            Some((
                key.clone(),
                Provenance {
                    author,
                    source_message_id: item["sourceMessageId"].as_str().map(str::to_string),
                    quote: item["quote"].as_str().map(str::to_string),
                    at: item["at"].as_str().unwrap_or("").to_string(),
                },
            ))
        })
        .collect()
}

fn reflection_of(value: &Value) -> Reflection {
    Reflection {
        last_position: value["lastPosition"].as_i64(),
        last_message_id: value["lastMessageId"].as_str().map(str::to_string),
        at: value["at"].as_str().map(str::to_string),
    }
}

fn temporal_map(value: &Value) -> BTreeMap<String, Temporal> {
    let Some(object) = value.as_object() else {
        return BTreeMap::new();
    };
    object
        .iter()
        .filter_map(|(key, item)| {
            if !item.is_object() {
                return None;
            }
            Some((
                key.clone(),
                Temporal {
                    from: item["from"].as_str().map(str::to_string),
                    to: item["to"].as_str().map(str::to_string),
                },
            ))
        })
        .collect()
}
