//! Journal filesystem restoration so a retry does not overwrite later edits.
use super::rooms_overlay::FIELDS;
use crate::platform::error::{ApiError, ApiResult};
use serde_json::{json, Map, Value};
use std::path::Path;

fn bindings(doc: &Value, ids: &Map<String, Value>) -> ApiResult<Map<String, Value>> {
    let rooms = doc["rooms"]
        .as_array()
        .ok_or_else(|| ApiError::new(500, "groups.json is not valid"))?;
    let mut result = Map::new();
    for id in ids.keys() {
        let room = rooms
            .iter()
            .find(|room| room["id"].as_str() == Some(id))
            .ok_or_else(|| {
                ApiError::new(
                    409,
                    "A group changed during local restore; both files were preserved",
                )
            })?;
        let mut fields = Map::new();
        for key in FIELDS {
            if let Some(v) = room.get(*key) {
                fields.insert((*key).into(), v.clone());
            }
        }
        result.insert(id.clone(), Value::Object(fields));
    }
    Ok(result)
}

pub(super) async fn restore(
    file: &Path,
    doc: Value,
    mut envelope: Map<String, Value>,
    mut local: Map<String, Value>,
) -> ApiResult<Value> {
    let pending = envelope.get("_hbotsRestore").cloned();
    let (base, target) = if let Some(journal) = pending {
        let base = journal
            .get("base")
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| ApiError::new(500, "Group local restore journal is invalid"))?;
        let target = journal
            .get("target")
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| ApiError::new(500, "Group local restore journal is invalid"))?;
        (base, target)
    } else {
        let mut ids = Map::new();
        for room in doc["rooms"].as_array().into_iter().flatten() {
            if let Some(id) = room["id"].as_str() {
                if local
                    .get(id)
                    .and_then(Value::as_object)
                    .is_some_and(|fields| FIELDS.iter().any(|key| fields.contains_key(*key)))
                {
                    ids.insert(id.into(), Value::Null);
                }
            }
        }
        if ids.is_empty() {
            return Ok(doc);
        }
        let base = bindings(&doc, &ids)?;
        let mut target = base.clone();
        for id in ids.keys() {
            let fields = target
                .get_mut(id)
                .and_then(Value::as_object_mut)
                .expect("bindings are maps");
            for key in FIELDS {
                if let Some(v) = local.get(id).and_then(|v| v.get(*key)) {
                    fields.insert((*key).into(), v.clone());
                }
            }
        }
        envelope.insert(
            "_hbotsRestore".into(),
            json!({"base":base, "target":target}),
        );
        crate::storage::write_json(
            &file.with_file_name("groups.local.json"),
            &Value::Object(envelope.clone()),
        )
        .await?;
        (base, target)
    };
    if base.keys().ne(target.keys())
        || base.values().chain(target.values()).any(|value| {
            value
                .as_object()
                .is_none_or(|fields| fields.keys().any(|key| !FIELDS.contains(&key.as_str())))
        })
        || target.iter().any(|(id, state)| {
            FIELDS.iter().any(|key| {
                local
                    .get(id)
                    .and_then(|value| value.get(*key))
                    .is_some_and(|value| state.get(*key) != Some(value))
            })
        })
    {
        return Err(ApiError::new(500, "Group local restore journal is invalid"));
    }
    let bytes = tokio::fs::read(file).await?;
    let mut current: Value = serde_json::from_slice(&bytes)
        .map_err(|_| ApiError::new(500, "groups.json is not valid"))?;
    let actual = bindings(&current, &target)?;
    for id in target.keys() {
        if actual[id] != base[id] && actual[id] != target[id] {
            return Err(ApiError::new(
                409,
                "Group runtime changed during local restore; both files were preserved",
            ));
        }
    }
    for room in current["rooms"].as_array_mut().expect("validated rooms") {
        let Some(id) = room["id"].as_str().map(str::to_owned) else {
            continue;
        };
        let Some(state) = target.get(&id).and_then(Value::as_object) else {
            continue;
        };
        let fields = room
            .as_object_mut()
            .ok_or_else(|| ApiError::new(500, "groups.json is not valid"))?;
        for key in FIELDS {
            fields.remove(*key);
            if let Some(v) = state.get(*key) {
                fields.insert((*key).into(), v.clone());
            }
        }
    }
    if actual != target {
        crate::storage::write_json(file, &current).await?;
    }
    for id in target.keys() {
        if let Some(fields) = local.get_mut(id).and_then(Value::as_object_mut) {
            for key in FIELDS {
                fields.remove(*key);
            }
        }
    }
    envelope.remove("_hbotsRestore");
    envelope.insert("version".into(), json!(1));
    envelope.insert("rooms".into(), Value::Object(local));
    crate::storage::write_json(
        &file.with_file_name("groups.local.json"),
        &Value::Object(envelope),
    )
    .await?;
    Ok(current)
}
