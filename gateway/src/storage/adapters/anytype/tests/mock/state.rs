//! In-memory Anytype: types, properties, objects, files, and the fault switches.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Clone)]
pub(super) struct Object {
    pub id: String,
    pub name: String,
    pub type_key: String,
    pub properties: Value,
    pub markdown: String,
    pub etag: String,
    pub archived: bool,
}

#[derive(Clone)]
pub(super) struct FileBlob {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub bytes: Vec<u8>,
    pub sha: String,
}

#[derive(Clone)]
struct Replay {
    fingerprint: String,
    status: u16,
    body: Value,
}

pub(super) struct MockState {
    pub objects: BTreeMap<String, Object>,
    pub files: BTreeMap<String, FileBlob>,
    pub types: Vec<Value>,
    pub properties: Vec<Value>,
    next: u64,
    replays: BTreeMap<String, Replay>,
    pub not_granted: bool,
    pub malformed: bool,
    pub etag_mismatch: bool,
    pub archived_writes: bool,
    pub status_override: Option<u16>,
    pub rate_limits_left: u32,
    pub retry_after_secs: u64,
    pub delay: Duration,
}

impl MockState {
    pub(super) fn new() -> Self {
        Self {
            objects: BTreeMap::new(),
            files: BTreeMap::new(),
            types: Vec::new(),
            properties: Vec::new(),
            next: 1,
            replays: BTreeMap::new(),
            not_granted: false,
            malformed: false,
            etag_mismatch: false,
            archived_writes: false,
            status_override: None,
            rate_limits_left: 0,
            retry_after_secs: 1,
            delay: Duration::ZERO,
        }
    }

    pub(super) fn seed_objects(&mut self, count: usize) {
        for index in 0..count {
            let id = self.alloc("obj");
            self.objects.insert(
                id.clone(),
                Object {
                    id,
                    name: format!("note-{index}"),
                    type_key: "page".into(),
                    properties: json!({}),
                    markdown: String::new(),
                    etag: "1".into(),
                    archived: false,
                },
            );
        }
    }

    pub(super) fn seed_types(&mut self, count: usize) {
        for index in 0..count {
            self.types.push(json!({
                "key": format!("type_{index}"),
                "name": format!("Type {index}"),
                "layout": "basic",
            }));
        }
    }

    pub(super) fn tear_file(&mut self, id: &str) -> bool {
        let Some(file) = self.files.get_mut(id) else {
            return false;
        };
        file.bytes = b"torn-by-peer".to_vec();
        true
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

    pub(super) fn create_object(&mut self, body: &Value) -> Value {
        let id = self.alloc("obj");
        let object = Object {
            id: id.clone(),
            name: body["name"].as_str().unwrap_or("").to_string(),
            type_key: body["type"].as_str().unwrap_or("page").to_string(),
            properties: body["properties"].clone(),
            markdown: body["markdown"].as_str().unwrap_or("").to_string(),
            etag: "1".into(),
            archived: false,
        };
        let view = object_json(&object);
        self.objects.insert(id, object);
        view
    }

    pub(super) fn patch_object(&mut self, id: &str, body: &Value) -> Option<Value> {
        let object = self.objects.get_mut(id)?;
        if let Some(ops) = body["ops"].as_array() {
            for op in ops {
                if op["op"] == "set_properties" {
                    if let Some(name) = op["set"]["name"].as_str() {
                        object.name = name.to_string();
                    }
                    if let Some(props) = op["set"].as_object() {
                        let properties = object.properties.as_object_mut();
                        if let Some(properties) = properties {
                            for (key, value) in props {
                                if key != "name" {
                                    properties.insert(key.clone(), value.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        object.etag = format!("{}", object.etag.parse::<u64>().unwrap_or(1) + 1);
        Some(object_json(object))
    }

    pub(super) fn add_type(&mut self, body: &Value) -> Value {
        let row = json!({
            "key": body["key"].as_str().unwrap_or("custom"),
            "name": body["name"].as_str().unwrap_or(""),
            "layout": body["layout"].as_str().unwrap_or("basic"),
        });
        self.types.push(row.clone());
        row
    }

    pub(super) fn add_property(&mut self, body: &Value) -> Value {
        let row = json!({
            "key": body["key"].as_str().unwrap_or("custom"),
            "name": body["name"].as_str().unwrap_or(""),
            "format": body["format"].as_str().unwrap_or("text"),
        });
        self.properties.push(row.clone());
        row
    }

    pub(super) fn add_file(&mut self, name: String, mime: String, bytes: Vec<u8>) -> Value {
        let id = self.alloc("file");
        let sha = sha_hex(&bytes);
        let size = bytes.len();
        let view = json!({
            "id": id,
            "name": name,
            "mime_type": mime,
            "size": size,
            "sha": sha,
        });
        self.files.insert(
            id,
            FileBlob {
                id: view["id"].as_str().unwrap_or_default().to_string(),
                name,
                mime,
                bytes,
                sha,
            },
        );
        view
    }

    fn alloc(&mut self, prefix: &str) -> String {
        let id = format!("{prefix}-{}", self.next);
        self.next += 1;
        id
    }
}

pub(super) fn object_json(object: &Object) -> Value {
    json!({
        "id": object.id,
        "name": object.name,
        "type": object.type_key,
        "properties": object.properties,
        "markdown": object.markdown,
        "etag": object.etag,
        "archived": object.archived,
    })
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
    json!({
        "data": slice,
        "total": total,
        "offset": offset,
        "limit": limit,
        "has_more": end < total,
    })
}
