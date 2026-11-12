//! Types, properties, and files. Keys come back normalized.
//! A display name with no explicit key is minted (`Spike Path` becomes `spike_path`).

use super::state::{snake_name, FileBlob, Reply};
use super::MockState;
use serde_json::{json, Value};

impl MockState {
    pub(super) fn add_type(&mut self, body: &Value) -> Reply {
        let key = self.finish_key(body["api_key"].as_str().unwrap_or(""));
        let id = self.alloc("type");
        let etag = self.next_etag();
        let mut created = Vec::new();
        let mut attached = Vec::new();
        if let Some(defs) = body["property_definitions"].as_array() {
            for def in defs {
                let name = def["name"].as_str().unwrap_or("");
                let format = def["format"].as_str().unwrap_or("text");
                let row = self.insert_property(name, None, format);
                created.push(json!({
                    "key": row["key"],
                    "name": row["name"],
                    "format": row["format"],
                }));
                attached.push(json!({
                    "property": row["name"],
                    "format": row["format"],
                }));
            }
        }
        self.types.push(json!({
            "id": id,
            "key": key,
            "name": body["name"].as_str().unwrap_or(""),
            "layout": body["layout"].as_str().unwrap_or("basic"),
            "blocks": [{ "id": "dataview", "type": "dataview", "properties": attached }],
        }));
        Reply {
            status: 201,
            body: json!({
                "id": id,
                "key": key,
                "etag": etag,
                "created": {"properties": created},
            }),
            etag: Some(etag),
        }
    }

    /// The type row, shaped like the app's `blocks` dataview response.
    pub(super) fn get_type(&self, key: &str) -> Reply {
        match self.types.iter().find(|row| row["key"] == key) {
            Some(row) => Reply {
                status: 200,
                body: row.clone(),
                etag: None,
            },
            None => Reply {
                status: 404,
                body: json!({ "error": "type not found" }),
                etag: None,
            },
        }
    }

    /// Replaces the type's property list like the app does on PATCH.
    pub(super) fn patch_type(&mut self, key: &str, body: &Value) -> Reply {
        let mut attached = Vec::new();
        if let Some(defs) = body["property_definitions"].as_array() {
            for def in defs {
                let name = def["name"].as_str().unwrap_or("");
                let format = def["format"].as_str().unwrap_or("text");
                self.insert_property(name, None, format);
                attached.push(json!({ "property": name, "format": format }));
            }
        }
        let Some(row) = self.types.iter_mut().find(|row| row["key"] == key) else {
            return Reply {
                status: 404,
                body: json!({ "error": "type not found" }),
                etag: None,
            };
        };
        row["blocks"] = json!([{ "id": "dataview", "type": "dataview", "properties": attached }]);
        Reply {
            status: 200,
            body: json!({ "key": key }),
            etag: None,
        }
    }

    pub(super) fn add_property(&mut self, body: &Value) -> Reply {
        let row = self.insert_property(
            body["name"].as_str().unwrap_or(""),
            body["key"].as_str().filter(|key| !key.is_empty()),
            body["format"].as_str().unwrap_or("text"),
        );
        Reply {
            status: 201,
            body: json!({"id": row["id"], "key": row["key"]}),
            etag: None,
        }
    }

    fn insert_property(&mut self, name: &str, key: Option<&str>, format: &str) -> Value {
        let key = match key {
            Some(key) => self.finish_key(key),
            None => self.finish_key(&snake_name(name)),
        };
        let id = self.alloc("prop");
        let row = json!({"id": id, "key": key, "name": name, "format": format});
        self.properties.push(row.clone());
        row
    }

    pub(super) fn add_file(
        &mut self,
        filename: String,
        declared_mime: String,
        bytes: Vec<u8>,
    ) -> Reply {
        let name = stored_name(&filename);
        let mime = stored_mime(&declared_mime);
        let id = self.alloc("file");
        let size = bytes.len();
        let sha = super::state::sha_hex(&bytes);
        self.files.insert(id.clone(), FileBlob { bytes, sha });
        Reply {
            status: 201,
            body: json!({"id": id, "name": name, "mime_type": mime, "size": size}),
            etag: None,
        }
    }
}

fn stored_name(filename: &str) -> String {
    match filename.rsplit_once('.') {
        Some((base, extension)) if !base.is_empty() && !extension.is_empty() => base.to_string(),
        _ => filename.to_string(),
    }
}

/// `text/*` comes back as `text/plain; charset=utf-8`. `application/octet-stream` stays.
fn stored_mime(declared: &str) -> String {
    let declared = declared.trim();
    if declared == "application/octet-stream" {
        return declared.to_string();
    }
    if declared.starts_with("text/") {
        return "text/plain; charset=utf-8".to_string();
    }
    if declared.is_empty() {
        return "application/octet-stream".to_string();
    }
    declared.to_string()
}
