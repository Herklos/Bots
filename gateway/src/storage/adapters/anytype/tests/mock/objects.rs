//! Object create, patch ops, archive, restore, and the peer writer.
//! Patch accepts only `{ops: [...]}` with the three ops measured live.

use super::state::{Object, Reply};
use super::MockState;
use serde_json::{json, Map, Value};

#[derive(Clone)]
pub(super) struct Block {
    pub id: String,
    pub kind: String,
    pub text: String,
}

impl MockState {
    pub(super) fn create_object(&mut self, body: &Value) -> Reply {
        let id = self.alloc("obj");
        let markdown = body["markdown"].as_str().unwrap_or("").to_string();
        let blocks = self.blocks_from_markdown(&markdown);
        let etag = self.next_etag();
        let object = Object {
            id: id.clone(),
            name: body["name"].as_str().unwrap_or("").to_string(),
            type_key: body["type"].as_str().unwrap_or("page").to_string(),
            properties: if body["properties"].is_object() {
                body["properties"].clone()
            } else {
                json!({})
            },
            markdown,
            blocks,
            etag: etag.clone(),
            archived: false,
            created_date: self.now(),
            last_modified: None,
        };
        let type_key = object.type_key.clone();
        self.objects.insert(id.clone(), object);
        Reply {
            status: 201,
            body: json!({"id": id, "type": type_key, "etag": etag}),
            etag: Some(etag),
        }
    }

    pub(super) fn document(&self, id: &str) -> Option<Reply> {
        let object = self.objects.get(id)?;
        Some(Reply {
            status: 200,
            body: document(object),
            etag: Some(object.etag.clone()),
        })
    }

    pub(super) fn patch(&mut self, id: &str, if_match: Option<&str>, body: &Value) -> Reply {
        if self.archived_writes {
            return archived();
        }
        if !ops_only(body) {
            return bad(400, "Patch body is ops only.");
        }
        let Some(ops) = body["ops"].as_array() else {
            return bad(400, "A patch carries 1 to 512 ops.");
        };
        if ops.is_empty() || ops.len() > 512 {
            return bad(400, "A patch carries 1 to 512 ops.");
        }
        let mut draft = {
            let Some(current) = self.objects.get(id) else {
                return missing();
            };
            if current.archived {
                return archived();
            }
            if self.etag_mismatch || if_match.is_some_and(|etag| etag != current.etag) {
                return etag_conflict(&current.etag);
            }
            current.clone()
        };
        let created = match apply_ops(self, &mut draft, ops) {
            Ok(created) => created,
            Err(reply) => return reply,
        };
        let etag = self.next_etag();
        let modified = self.touch();
        draft.etag = etag.clone();
        draft.last_modified = Some(modified);
        let type_key = draft.type_key.clone();
        self.objects.insert(id.to_string(), draft);
        let mut response = json!({"id": id, "type": type_key, "etag": etag});
        if let Some(created) = created {
            response["created_blocks"] = Value::Object(created);
        }
        Reply {
            status: 200,
            body: response,
            etag: Some(etag),
        }
    }

    pub(super) fn archive(&mut self, space: &str, id: &str) -> Reply {
        if self.archived_writes {
            return archived();
        }
        let (object_id, type_key, already) = {
            let Some(object) = self.objects.get(id) else {
                return missing();
            };
            (object.id.clone(), object.type_key.clone(), object.archived)
        };
        if already {
            return archived();
        }
        let modified = self.touch();
        if let Some(object) = self.objects.get_mut(id) {
            object.archived = true;
            object.last_modified = Some(modified);
        }
        Reply {
            status: 200,
            body: json!({
                "id": object_id,
                "type": type_key,
                "warnings": [{
                    "path": "object_id",
                    "message": "moved to the Bin",
                    "hint": format!("restore it with POST /v2/spaces/{space}/objects/{object_id}/restore"),
                }],
            }),
            etag: None,
        }
    }

    pub(super) fn delete_permanent(&mut self, id: &str) -> Reply {
        if self.archived_writes {
            return archived();
        }
        let Some(object) = self.objects.remove(id) else {
            return missing();
        };
        Reply {
            status: 200,
            body: json!({"id": object.id, "type": object.type_key}),
            etag: None,
        }
    }

    pub(super) fn restore(&mut self, id: &str) -> Reply {
        if self.archived_writes {
            return archived();
        }
        let (object_id, type_key) = {
            let Some(object) = self.objects.get(id) else {
                return missing();
            };
            (object.id.clone(), object.type_key.clone())
        };
        let modified = self.touch();
        if let Some(object) = self.objects.get_mut(id) {
            object.archived = false;
            object.last_modified = Some(modified);
        }
        Reply {
            status: 200,
            body: json!({"id": object_id, "type": type_key}),
            etag: None,
        }
    }

    /// Another machine's write. Each property keeps the last value.
    /// `tear` keeps the previous `hb_sha` while the other properties change.
    pub(super) fn peer_write(&mut self, id: &str, update: &Value, tear: bool) -> bool {
        let Some(incoming) = update.as_object() else {
            return false;
        };
        if !self.objects.contains_key(id) {
            return false;
        }
        let etag = self.next_etag();
        let modified = self.touch();
        let Some(object) = self.objects.get_mut(id) else {
            return false;
        };
        if !object.properties.is_object() {
            object.properties = json!({});
        }
        let old_sha = object.properties.get("hb_sha").cloned();
        if let Some(props) = object.properties.as_object_mut() {
            for (key, value) in incoming {
                props.insert(key.clone(), value.clone());
            }
            if tear {
                match old_sha {
                    Some(sha) => {
                        props.insert("hb_sha".to_string(), sha);
                    }
                    None => {
                        props.remove("hb_sha");
                    }
                }
            }
        }
        object.etag = etag;
        object.last_modified = Some(modified);
        true
    }

    fn blocks_from_markdown(&mut self, markdown: &str) -> Vec<Block> {
        if markdown.is_empty() {
            return Vec::new();
        }
        vec![Block {
            id: self.alloc("blk"),
            kind: "paragraph".to_string(),
            text: markdown.to_string(),
        }]
    }
}

fn apply_ops(
    state: &mut MockState,
    draft: &mut Object,
    ops: &[Value],
) -> Result<Option<Map<String, Value>>, Reply> {
    let mut created: Option<Map<String, Value>> = None;
    for op in ops {
        match op["op"].as_str() {
            Some("set_properties") => set_properties(draft, op),
            Some("insert_blocks") => {
                let map = created.get_or_insert_with(Map::new);
                insert_blocks(state, draft, op, map);
            }
            Some("replace_text") => replace_text(draft, op)?,
            _ => return Err(bad(400, "Unknown patch op.")),
        }
    }
    Ok(created)
}

fn set_properties(draft: &mut Object, op: &Value) {
    if !draft.properties.is_object() {
        draft.properties = json!({});
    }
    let Some(props) = draft.properties.as_object_mut() else {
        return;
    };
    if let Some(set) = op["set"].as_object() {
        for (key, value) in set {
            props.insert(key.clone(), value.clone());
        }
    }
    if let Some(add) = op["add"].as_object() {
        for (key, value) in add {
            match props.get_mut(key) {
                Some(Value::Array(items)) => {
                    if let Some(extra) = value.as_array() {
                        items.extend(extra.clone());
                    } else {
                        items.push(value.clone());
                    }
                }
                _ => {
                    props.insert(key.clone(), value.clone());
                }
            }
        }
    }
    if let Some(remove) = op["remove"].as_object() {
        for (key, value) in remove {
            if let Some(Value::Array(items)) = props.get_mut(key) {
                items.retain(|item| match value {
                    Value::Array(drop) => !drop.contains(item),
                    other => item != other,
                });
            } else if props.get(key) == Some(value) {
                props.remove(key);
            }
        }
    }
    if let Some(unset) = op["unset"].as_array() {
        for key in unset {
            if let Some(key) = key.as_str() {
                props.remove(key);
            }
        }
    }
}

fn insert_blocks(
    state: &mut MockState,
    draft: &mut Object,
    op: &Value,
    created: &mut Map<String, Value>,
) {
    let Some(blocks) = op["blocks"].as_array() else {
        return;
    };
    for (index, block) in blocks.iter().enumerate() {
        let id = state.alloc("blk");
        let text = block["text"].as_str().unwrap_or("").to_string();
        let kind = block["type"].as_str().unwrap_or("paragraph").to_string();
        let key = block
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| index.to_string());
        created.insert(key, Value::String(id.clone()));
        if !draft.markdown.is_empty() && !text.is_empty() {
            draft.markdown.push_str("\n\n");
        }
        draft.markdown.push_str(&text);
        draft.blocks.push(Block { id, kind, text });
    }
}

fn replace_text(draft: &mut Object, op: &Value) -> Result<(), Reply> {
    let find = op["find"].as_str().unwrap_or("");
    let replace = op["replace"].as_str().unwrap_or("");
    if find.is_empty() || !draft.markdown.contains(find) {
        return Err(missing());
    }
    draft.markdown = draft.markdown.replace(find, replace);
    for block in &mut draft.blocks {
        block.text = block.text.replace(find, replace);
    }
    Ok(())
}

fn ops_only(body: &Value) -> bool {
    body.as_object()
        .is_some_and(|map| map.len() == 1 && map.contains_key("ops"))
}

pub(super) fn document(object: &Object) -> Value {
    let mut body = json!({
        "id": object.id,
        "name": object.name,
        "type": object.type_key,
        "properties": object.properties,
        "blocks": object.blocks.iter().map(|block| json!({
            "id": block.id,
            "type": block.kind,
            "text": block.text,
        })).collect::<Vec<_>>(),
        "created_date": object.created_date,
    });
    if let Some(modified) = &object.last_modified {
        body["last_modified_date"] = json!(modified);
    }
    body
}

fn etag_conflict(etag: &str) -> Reply {
    Reply {
        status: 409,
        body: json!({
            "code": "etag_mismatch",
            "message": format!("The object changed. current etag is \"{etag}\"; re-read the object and retry"),
        }),
        etag: None,
    }
}

fn archived() -> Reply {
    Reply {
        status: 409,
        body: json!({
            "code": "object_archived",
            "message": "The object is in the Bin and cannot be edited until it is restored",
        }),
        etag: None,
    }
}

fn missing() -> Reply {
    Reply {
        status: 404,
        body: json!({"code": "not_found", "message": "missing"}),
        etag: None,
    }
}

fn bad(status: u16, message: &str) -> Reply {
    Reply {
        status,
        body: json!({"message": message}),
        etag: None,
    }
}
