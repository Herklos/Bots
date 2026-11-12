//! The local sync manifest. It lives outside the Bots folder.
//! A different host, space, or root drops the entries so the next run starts clean.
//! A newer `version` is left on disk untouched.

use crate::storage::write_atomic;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::io;
use std::path::Path;

pub(super) const VERSION: u32 = 1;

const TOP_KEYS: &[&str] = &[
    "version",
    "host",
    "space_id",
    "root",
    "device_id",
    "entries",
];

const ENTRY_KEYS: &[&str] = &[
    "base_sha",
    "object_id",
    "remote_etag",
    "remote_modified",
    "local_mtime_ns",
    "local_size",
    "tombstone",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Identity {
    pub host: String,
    pub space_id: String,
    pub root: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Entry {
    pub base_sha: Option<String>,
    pub object_id: Option<String>,
    pub remote_etag: Option<String>,
    pub remote_modified: Option<String>,
    pub local_mtime_ns: Option<i64>,
    pub local_size: Option<u64>,
    pub tombstone: bool,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Manifest {
    pub version: u32,
    pub host: String,
    pub space_id: String,
    pub root: String,
    pub device_id: String,
    pub entries: BTreeMap<String, Entry>,
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Loaded {
    Missing,
    Current(Manifest),
    /// Host, space, or root changed. Entries are gone. The device id stays.
    Reset(Manifest),
}

#[derive(Debug)]
pub(super) enum ManifestError {
    Io(io::Error),
    Corrupt(String),
    FutureVersion(u64),
}

impl Manifest {
    pub(super) fn fresh(identity: &Identity, device_id: &str) -> Self {
        Self {
            version: VERSION,
            host: identity.host.clone(),
            space_id: identity.space_id.clone(),
            root: identity.root.clone(),
            device_id: device_id.to_string(),
            entries: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }
}

pub(super) fn load(path: &Path, identity: &Identity) -> Result<Loaded, ManifestError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Loaded::Missing),
        Err(error) => return Err(ManifestError::Io(error)),
    };
    let manifest = parse_document(&text)?;
    if manifest.host != identity.host
        || manifest.space_id != identity.space_id
        || manifest.root != identity.root
    {
        return Ok(Loaded::Reset(Manifest::fresh(
            identity,
            &manifest.device_id,
        )));
    }
    Ok(Loaded::Current(manifest))
}

pub(super) async fn save(path: &Path, manifest: &Manifest) -> Result<(), ManifestError> {
    let text = serde_json::to_string_pretty(&document_value(manifest)).expect("manifest is json");
    write_atomic(path, format!("{text}\n").as_bytes(), Some(0o600))
        .await
        .map_err(ManifestError::Io)
}

fn parse_document(text: &str) -> Result<Manifest, ManifestError> {
    let Value::Object(mut obj) =
        serde_json::from_str(text).map_err(|err| corrupt(err.to_string()))?
    else {
        return Err(corrupt("manifest is not an object"));
    };
    let version = match obj.remove("version") {
        None => 0,
        Some(Value::Number(number)) => number
            .as_u64()
            .ok_or_else(|| corrupt("version is not a whole number"))?,
        Some(_) => return Err(corrupt("version is not a number")),
    };
    if version > u64::from(VERSION) {
        return Err(ManifestError::FutureVersion(version));
    }
    let host = require_string(&mut obj, "host")?;
    let space_id = require_string(&mut obj, "space_id")?;
    let root = require_string(&mut obj, "root")?;
    let device_id = require_string(&mut obj, "device_id")?;
    let entries = match obj.remove("entries") {
        None => BTreeMap::new(),
        Some(Value::Object(map)) => parse_entries(map)?,
        Some(_) => return Err(corrupt("entries is not an object")),
    };
    Ok(Manifest {
        version: VERSION,
        host,
        space_id,
        root,
        device_id,
        entries,
        extra: extras(obj, TOP_KEYS),
    })
}

fn parse_entries(map: Map<String, Value>) -> Result<BTreeMap<String, Entry>, ManifestError> {
    let mut entries = BTreeMap::new();
    for (path, value) in map {
        let Value::Object(mut obj) = value else {
            return Err(corrupt(format!("entry {path} is not an object")));
        };
        let entry = Entry {
            base_sha: optional_string(&mut obj, "base_sha")?,
            object_id: optional_string(&mut obj, "object_id")?,
            remote_etag: optional_string(&mut obj, "remote_etag")?,
            remote_modified: optional_string(&mut obj, "remote_modified")?,
            local_mtime_ns: optional_i64(&mut obj, "local_mtime_ns")?,
            local_size: optional_u64(&mut obj, "local_size")?,
            tombstone: optional_bool(&mut obj, "tombstone")?.unwrap_or(false),
            extra: extras(obj, ENTRY_KEYS),
        };
        entries.insert(path, entry);
    }
    Ok(entries)
}

fn document_value(manifest: &Manifest) -> Value {
    let mut obj = Map::new();
    obj.insert("version".to_string(), Value::from(VERSION));
    obj.insert("host".to_string(), Value::from(manifest.host.clone()));
    obj.insert(
        "space_id".to_string(),
        Value::from(manifest.space_id.clone()),
    );
    obj.insert("root".to_string(), Value::from(manifest.root.clone()));
    obj.insert(
        "device_id".to_string(),
        Value::from(manifest.device_id.clone()),
    );
    let mut entries = Map::new();
    for (path, entry) in &manifest.entries {
        entries.insert(path.clone(), entry_value(entry));
    }
    obj.insert("entries".to_string(), Value::Object(entries));
    insert_extras(&mut obj, &manifest.extra, TOP_KEYS);
    Value::Object(obj)
}

fn entry_value(entry: &Entry) -> Value {
    let mut obj = Map::new();
    insert_opt(&mut obj, "base_sha", entry.base_sha.clone());
    insert_opt(&mut obj, "object_id", entry.object_id.clone());
    insert_opt(&mut obj, "remote_etag", entry.remote_etag.clone());
    insert_opt(&mut obj, "remote_modified", entry.remote_modified.clone());
    if let Some(mtime) = entry.local_mtime_ns {
        obj.insert("local_mtime_ns".to_string(), Value::from(mtime));
    }
    if let Some(size) = entry.local_size {
        obj.insert("local_size".to_string(), Value::from(size));
    }
    obj.insert("tombstone".to_string(), Value::from(entry.tombstone));
    insert_extras(&mut obj, &entry.extra, ENTRY_KEYS);
    Value::Object(obj)
}

fn insert_opt(obj: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        obj.insert(key.to_string(), Value::from(value));
    }
}

fn insert_extras(obj: &mut Map<String, Value>, extra: &BTreeMap<String, Value>, known: &[&str]) {
    for (key, value) in extra {
        if !known.contains(&key.as_str()) {
            obj.insert(key.clone(), value.clone());
        }
    }
}

fn extras(obj: Map<String, Value>, known: &[&str]) -> BTreeMap<String, Value> {
    obj.into_iter()
        .filter(|(key, _)| !known.contains(&key.as_str()))
        .collect()
}

fn require_string(obj: &mut Map<String, Value>, key: &str) -> Result<String, ManifestError> {
    match obj.remove(key) {
        Some(Value::String(value)) => Ok(value),
        Some(_) => Err(corrupt(format!("{key} is not a string"))),
        None => Err(corrupt(format!("{key} is missing"))),
    }
}

fn optional_string(
    obj: &mut Map<String, Value>,
    key: &str,
) -> Result<Option<String>, ManifestError> {
    match obj.remove(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(corrupt(format!("{key} is not a string"))),
    }
}

fn optional_i64(obj: &mut Map<String, Value>, key: &str) -> Result<Option<i64>, ManifestError> {
    match obj.remove(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_i64()
            .map(Some)
            .ok_or_else(|| corrupt(format!("{key} is not an i64"))),
        Some(_) => Err(corrupt(format!("{key} is not a number"))),
    }
}

fn optional_u64(obj: &mut Map<String, Value>, key: &str) -> Result<Option<u64>, ManifestError> {
    match obj.remove(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_u64()
            .map(Some)
            .ok_or_else(|| corrupt(format!("{key} is not a u64"))),
        Some(_) => Err(corrupt(format!("{key} is not a number"))),
    }
}

fn optional_bool(obj: &mut Map<String, Value>, key: &str) -> Result<Option<bool>, ManifestError> {
    match obj.remove(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(value)),
        Some(_) => Err(corrupt(format!("{key} is not a boolean"))),
    }
}

fn corrupt(message: impl Into<String>) -> ManifestError {
    ManifestError::Corrupt(message.into())
}
