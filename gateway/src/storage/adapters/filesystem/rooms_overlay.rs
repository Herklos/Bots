//! Per-device room runtime, separate from shareable room metadata and messages.
use crate::platform::error::{ApiError, ApiResult};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub(super) const FIELDS: &[&str] = &["threads", "anchors", "hops", "lastSeenAt"];
fn path(file: &Path) -> PathBuf {
    file.with_file_name("groups.local.json")
}

async fn read(file: &Path) -> ApiResult<(Map<String, Value>, Map<String, Value>)> {
    match tokio::fs::read(path(file)).await {
        Ok(bytes) => {
            let value: Value = serde_json::from_slice(&bytes)
                .map_err(|_| ApiError::new(500, "groups.local.json is not valid"))?;
            if value.get("version") != Some(&json!(1)) {
                return Err(ApiError::new(
                    500,
                    "groups.local.json uses an unsupported version",
                ));
            }
            let rooms = value
                .get("rooms")
                .and_then(Value::as_object)
                .cloned()
                .ok_or_else(|| ApiError::new(500, "groups.local.json is not valid"))?;
            Ok((value.as_object().cloned().unwrap_or_default(), rooms))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((Map::new(), Map::new())),
        Err(e) => Err(e.into()),
    }
}

pub(super) async fn load(file: &Path, mut doc: Value, split: bool) -> ApiResult<Value> {
    let (envelope, mut local) = read(file).await?;
    if !split {
        return super::rooms_restore::restore(file, doc, envelope, local).await;
    }
    if envelope.contains_key("_hbotsRestore") {
        doc = super::rooms_restore::restore(file, doc, envelope, local).await?;
        local = read(file).await?.1;
    }
    let mut changed = false;
    if let Some(rooms) = doc.get_mut("rooms").and_then(Value::as_array_mut) {
        for room in rooms {
            let Some(id) = room["id"].as_str().map(str::to_owned) else {
                if split {
                    return Err(ApiError::new(
                        500,
                        "groups.json contains a group without an id",
                    ));
                }
                continue;
            };
            let Some(fields) = room.as_object_mut() else {
                continue;
            };
            for key in FIELDS {
                if split {
                    changed |= fields.contains_key(*key);
                }
                if let Some(v) = local.get(&id).and_then(|r| r.get(*key)) {
                    fields.insert((*key).into(), v.clone());
                    if !split {
                        changed = true;
                    }
                }
            }
        }
    }
    if changed {
        save(file, &doc, true).await?;
    }
    Ok(doc)
}

pub(super) async fn save(file: &Path, doc: &Value, split: bool) -> ApiResult<()> {
    if !split {
        crate::storage::write_json(file, doc).await?;
        return Ok(());
    }
    let mut shared = doc.clone();
    let (mut envelope, mut local) = read(file).await?;
    if let Some(rooms) = shared.get_mut("rooms").and_then(Value::as_array_mut) {
        for room in rooms {
            let Some(id) = room["id"].as_str().map(str::to_owned) else {
                return Err(ApiError::new(
                    500,
                    "groups.json contains a group without an id",
                ));
            };
            let Some(fields) = room.as_object_mut() else {
                continue;
            };
            let mut state = local
                .get(&id)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            for key in FIELDS {
                state.remove(*key);
                if let Some(value) = fields.remove(*key) {
                    state.insert((*key).into(), value);
                }
            }
            local.insert(id, Value::Object(state));
        }
    }
    envelope.remove("_hbotsRestore");
    envelope.insert("version".into(), json!(1));
    envelope.insert("rooms".into(), Value::Object(local));
    crate::storage::write_json(&path(file), &Value::Object(envelope)).await?;
    crate::storage::write_json(file, &shared).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn split_and_restore_preserve_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("groups.json");
        let doc = json!({"rooms":[{"id":"g", "threads":{"ada":"t"}, "hops":2}]});
        crate::storage::write_json(&file, &doc).await.unwrap();
        assert_eq!(load(&file, doc, true).await.unwrap()["rooms"][0]["hops"], 2);
        let shared: Value = serde_json::from_slice(&tokio::fs::read(&file).await.unwrap()).unwrap();
        assert!(shared["rooms"][0].get("threads").is_none());
        assert_eq!(
            load(&file, shared, false).await.unwrap()["rooms"][0]["threads"]["ada"],
            "t"
        );
        let restored: Value =
            serde_json::from_slice(&tokio::fs::read(&file).await.unwrap()).unwrap();
        assert_eq!(restored["rooms"][0]["hops"], 2);
    }

    #[tokio::test]
    async fn interrupted_split_prefers_written_local_values_and_keeps_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("groups.json");
        let doc = json!({"rooms":[{"id":"g", "threads":{"ada":"old"}}]});
        crate::storage::write_json(&file, &doc).await.unwrap();
        crate::storage::write_json(
            &path(&file),
            &json!({"version":1,"futureTop":"keep", "rooms":{"g":{"threads":{"ada":"new"},"future":"keep"}}}),
        )
        .await
        .unwrap();
        assert_eq!(
            load(&file, doc, true).await.unwrap()["rooms"][0]["threads"]["ada"],
            "new"
        );
        let (envelope, local) = read(&file).await.unwrap();
        assert_eq!(envelope["futureTop"], "keep");
        assert_eq!(local["g"]["future"], "keep");
    }

    #[tokio::test]
    async fn refuses_unknown_overlay_version_without_changing_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("groups.json");
        let shared = json!({"rooms":[{"id":"g","hops":1}]});
        let future = json!({"version":2,"rooms":{"g":{"hops":5}}});
        crate::storage::write_json(&file, &shared).await.unwrap();
        crate::storage::write_json(&path(&file), &future)
            .await
            .unwrap();
        assert!(load(&file, shared.clone(), true).await.is_err());
        assert!(load(&file, shared.clone(), false).await.is_err());
        assert_eq!(
            serde_json::from_slice::<Value>(&tokio::fs::read(&file).await.unwrap()).unwrap(),
            shared
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&tokio::fs::read(path(&file)).await.unwrap()).unwrap(),
            future
        );
    }

    #[tokio::test]
    async fn room_list_reads_migrated_runtime_in_anytype_mode() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("groups.json");
        crate::storage::write_json(
            &file,
            &json!({"rooms":[{"id":"g", "threads":{"ada":"t"}, "hops":2}]}),
        )
        .await
        .unwrap();
        let rooms = crate::chat::rooms::Rooms::new(file.clone()).with_local_overlay();
        assert_eq!(rooms.list().await.unwrap()[0]["threads"]["ada"], "t");
        let shared: Value = serde_json::from_slice(&tokio::fs::read(&file).await.unwrap()).unwrap();
        assert!(shared["rooms"][0].get("threads").is_none());
        assert_eq!(rooms.get("g").await.unwrap()["hops"], 2);
    }

    #[tokio::test]
    async fn refuses_to_split_a_room_without_an_identity() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("groups.json");
        let original = json!({"rooms":[{"threads":{"ada":"t"}}]});
        crate::storage::write_json(&file, &original).await.unwrap();
        assert!(load(&file, original.clone(), true).await.is_err());
        let unchanged: Value =
            serde_json::from_slice(&tokio::fs::read(file).await.unwrap()).unwrap();
        assert_eq!(unchanged, original);
    }

    #[tokio::test]
    async fn independent_room_stores_do_not_lose_concurrent_replies() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("groups.json");
        crate::storage::write_json(&file, &json!({"rooms":[{"id":"g", "messages":[]}]}))
            .await
            .unwrap();
        let a = std::sync::Arc::new(crate::chat::rooms::Rooms::new(file.clone()).with_local_overlay());
        let b = std::sync::Arc::new(crate::chat::rooms::Rooms::new(file).with_local_overlay());
        let mut tasks = Vec::new();
        for i in 0..20 {
            let rooms = if i % 2 == 0 { a.clone() } else { b.clone() };
            tasks.push(tokio::spawn(async move {
                rooms
                    .reply("g", "ada", "Ada", "hello", &format!("id-{i}"), "2026-01-01")
                    .await
                    .unwrap();
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(
            a.get("g").await.unwrap()["messages"]
                .as_array()
                .unwrap()
                .len(),
            20
        );
    }

    #[tokio::test]
    async fn interrupted_restore_preserves_later_group_runtime_edits() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("groups.json");
        let shared = json!({"rooms":[{"id":"g", "hops":3}]});
        let local = json!({"version":1,"rooms":{"g":{"hops":2}}, "_hbotsRestore": {
            "base":{"g":{}}, "target":{"g":{"hops":2}}
        }});
        crate::storage::write_json(&file, &shared).await.unwrap();
        crate::storage::write_json(&path(&file), &local)
            .await
            .unwrap();
        assert_eq!(
            load(&file, shared.clone(), false)
                .await
                .unwrap_err()
                .status
                .as_u16(),
            409
        );
        assert_eq!(
            load(&file, shared.clone(), true)
                .await
                .unwrap_err()
                .status
                .as_u16(),
            409
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&tokio::fs::read(&file).await.unwrap()).unwrap(),
            shared
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&tokio::fs::read(path(&file)).await.unwrap()).unwrap(),
            local
        );
    }

    #[tokio::test]
    async fn interrupted_restore_finishes_when_group_target_was_already_written() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("groups.json");
        let shared = json!({"rooms":[{"id":"g", "hops":2}]});
        let local = json!({"version":1,"rooms":{"g":{"hops":2}}, "_hbotsRestore": {
            "base":{"g":{}}, "target":{"g":{"hops":2}}
        }});
        crate::storage::write_json(&file, &shared).await.unwrap();
        crate::storage::write_json(&path(&file), &local)
            .await
            .unwrap();
        assert_eq!(load(&file, shared.clone(), false).await.unwrap(), shared);
        assert!(!read(&file).await.unwrap().0.contains_key("_hbotsRestore"));
    }
}
