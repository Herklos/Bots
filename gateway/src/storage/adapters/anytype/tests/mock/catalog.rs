//! Types, properties, and files. Keys come back normalized.
//! A display name with no explicit key is minted (`Spike Path` becomes `spike_path`).

use super::state::{snake_name, FileBlob, Reply};
use super::MockState;
use serde_json::{json, Value};

impl MockState {
    pub(super) fn add_type(&mut self, body: &Value) -> Reply {
        if let Some(error) = reject_type_body(body) {
            return error;
        }
        let key = self.finish_key(body["api_key"].as_str().unwrap_or(""));
        let id = self.alloc("type");
        let etag = self.next_etag();
        let mut created = Vec::new();
        let mut attached = Vec::new();
        if let Some(defs) = body["property_definitions"].as_array() {
            for def in defs {
                let row = match def["property"].as_str() {
                    Some(key) => match self.properties.iter().find(|row| row["key"] == key) {
                        Some(row) => row.clone(),
                        None => match builtin_property(key) {
                            Some(row) => row,
                            None => return unknown_property(key),
                        },
                    },
                    None => {
                        let name = def["name"].as_str().unwrap_or("");
                        let format = def["format"].as_str().unwrap_or("text");
                        let row = self.insert_property(name, None, format);
                        created.push(json!({
                            "key": row["key"],
                            "name": row["name"],
                            "format": row["format"],
                        }));
                        row
                    }
                };
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

    /// True when a search by this type key would be refused by the app.
    pub(super) fn knows_type(&self, key: &str) -> bool {
        self.types.iter().any(|row| row["key"] == key)
            || self.objects.values().any(|object| object.type_key == key)
    }

    /// The live app 400s on a `type` or a `fields` entry it has never heard of.
    pub(super) fn reject_unknown(&self, body: &Value) -> Option<Value> {
        if let Some(key) = body["type"].as_str().filter(|key| !key.is_empty()) {
            if !self.knows_type(key) {
                return Some(json!({
                    "code": "validation_failed",
                    "message": format!("type \"{key}\" not found"),
                }));
            }
        }
        let unknown = body["fields"]
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .find(|key| !self.knows_property(key))?;
        Some(json!({
            "code": "validation_failed",
            "message": format!("unknown property key \"{unknown}\""),
        }))
    }

    pub(super) fn knows_property(&self, key: &str) -> bool {
        const TOP: [&str; 5] = ["id", "name", "type", "created_date", "last_modified_date"];
        TOP.contains(&key)
            || self.properties.iter().any(|row| row["key"] == key)
            || self
                .objects
                .values()
                .any(|object| object.properties.get(key).is_some())
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
        if let Some(ops) = body["ops"].as_array() {
            let mut additions = Vec::new();
            for op in ops {
                if op["op"] != "add_property" {
                    return invalid("unsupported type operation");
                }
                let property = op["property"].as_str().unwrap_or("");
                let row = self
                    .properties
                    .iter()
                    .find(|row| row["key"] == property || row["name"] == property)
                    .cloned()
                    .or_else(|| builtin_property(property));
                let Some(row) = row else {
                    return unknown_property(property);
                };
                additions.push(json!({"property": row["name"], "format": row["format"]}));
            }
            let Some(row) = self.types.iter_mut().find(|row| row["key"] == key) else {
                return Reply {
                    status: 404,
                    body: json!({"error":"type not found"}),
                    etag: None,
                };
            };
            let Some(existing) = row["blocks"][0]["properties"].as_array_mut() else {
                return invalid("invalid properties");
            };
            for addition in additions {
                if !existing
                    .iter()
                    .any(|p| p["property"] == addition["property"])
                {
                    existing.push(addition);
                }
            }
            return Reply {
                status: 200,
                body: json!({"key":key}),
                etag: None,
            };
        }
        let mut attached = Vec::new();
        if let Some(defs) = body["property_definitions"].as_array() {
            for def in defs {
                let row = match def["property"].as_str() {
                    Some(key) => match self.properties.iter().find(|row| row["key"] == key) {
                        Some(row) => row.clone(),
                        None => match builtin_property(key) {
                            Some(row) => row,
                            None => return unknown_property(key),
                        },
                    },
                    None => {
                        let name = def["name"].as_str().unwrap_or("");
                        let format = def["format"].as_str().unwrap_or("text");
                        self.insert_property(name, None, format)
                    }
                };
                attached.push(json!({ "property": row["name"], "format": row["format"] }));
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
        if body["name"].as_str().is_none_or(str::is_empty) || body["format"].as_str().is_none() {
            return invalid("name and format are required");
        }
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

fn builtin_property(key: &str) -> Option<Value> {
    if crate::storage::adapters::anytype::schema::table::BUILTINS.contains(&key) {
        Some(json!({"key":key,"name":key,"format":"builtin"}))
    } else {
        None
    }
}

fn invalid(message: &str) -> Reply {
    Reply {
        status: 400,
        body: json!({ "code": "validation_failed", "message": message }),
        etag: None,
    }
}

/// The live API refuses `key` (the field is `api_key`) and any other member.
fn reject_type_body(body: &Value) -> Option<Reply> {
    const ALLOWED: [&str; 5] = [
        "name",
        "api_key",
        "layout",
        "plural_name",
        "property_definitions",
    ];
    if body["name"].as_str().is_none_or(str::is_empty) {
        return Some(invalid("name is required"));
    }
    let extra = body
        .as_object()?
        .keys()
        .find(|key| !ALLOWED.contains(&key.as_str()))?;
    Some(invalid(&format!("unknown field \"{extra}\"")))
}

fn unknown_property(key: &str) -> Reply {
    Reply {
        status: 400,
        body: json!({
            "code": "validation_failed",
            "message": format!("unknown property key \"{key}\""),
        }),
        etag: None,
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
