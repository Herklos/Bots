//! Separate machine bindings from a Bot profile in Anytype mode.
use crate::platform::error::{ApiError, ApiResult};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

const FIELDS: &[&str] = &[
    "projectId",
    "mainThreadId",
    "lastSeenAt",
    "environment",
    "scheduleTaskIds",
];

fn path(profile: &Path) -> PathBuf {
    profile.with_file_name("local.json")
}

async fn read(profile: &Path) -> ApiResult<Map<String, Value>> {
    match tokio::fs::read(path(profile)).await {
        Ok(bytes) => {
            let fields = serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|v| v.as_object().cloned())
                .ok_or_else(|| ApiError::new(500, "Bot local.json is not valid"))?;
            if fields
                .get("_hbotsOverlayVersion")
                .is_some_and(|v| *v != Value::from(1))
            {
                return Err(ApiError::new(
                    500,
                    "Bot local.json uses an unsupported version",
                ));
            }
            Ok(fields)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(e) => Err(e.into()),
    }
}

pub(super) async fn load(profile: &Path, mut value: Value) -> ApiResult<Value> {
    let mut local = read(profile).await?;
    if local.contains_key("_hbotsRestore") {
        value = restore(profile, value).await?;
        local = read(profile).await?;
    }
    let fields = value
        .as_object_mut()
        .ok_or_else(|| ApiError::new(500, "bot.json is not valid"))?;
    let migrate = FIELDS.iter().any(|key| fields.contains_key(*key));
    for key in FIELDS {
        if let Some(v) = local.get(*key) {
            fields.insert((*key).into(), v.clone());
        }
    }
    if migrate {
        save(profile, &value).await?;
    }
    Ok(value)
}

/// Restore bindings when switching back to the filesystem backend. The journal
/// makes an interrupted restore distinguishable from a later legacy edit.
pub(super) async fn restore(profile: &Path, value: Value) -> ApiResult<Value> {
    let mut local = read(profile).await?;
    if local.get("_hbotsOverlayVersion") != Some(&Value::from(1)) {
        return Ok(value);
    }
    let pending = local.get("_hbotsRestore").cloned();
    if pending.is_none() && !FIELDS.iter().any(|key| local.contains_key(*key)) {
        return Ok(value);
    }
    let (base, target) = if let Some(journal) = pending {
        let base = journal
            .get("base")
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| ApiError::new(500, "Bot local restore journal is invalid"))?;
        let target = journal
            .get("target")
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| ApiError::new(500, "Bot local restore journal is invalid"))?;
        (base, target)
    } else {
        let base = bindings(&value)?;
        let mut target = base.clone();
        for key in FIELDS {
            if let Some(v) = local.get(*key) {
                target.insert((*key).into(), v.clone());
            }
        }
        local.insert(
            "_hbotsRestore".into(),
            serde_json::json!({"base": base, "target": target}),
        );
        crate::storage::write_json(&path(profile), &Value::Object(local.clone())).await?;
        (base, target)
    };
    if base
        .keys()
        .chain(target.keys())
        .any(|key| !FIELDS.contains(&key.as_str()))
        || FIELDS.iter().any(|key| {
            local
                .get(*key)
                .is_some_and(|value| target.get(*key) != Some(value))
        })
    {
        return Err(ApiError::new(500, "Bot local restore journal is invalid"));
    }
    let bytes = tokio::fs::read(profile).await?;
    let mut current: Value =
        serde_json::from_slice(&bytes).map_err(|_| ApiError::new(500, "bot.json is not valid"))?;
    let actual = bindings(&current)?;
    if actual != base && actual != target {
        return Err(ApiError::new(
            409,
            "Bot bindings changed during local restore; both files were preserved",
        ));
    }
    if actual != target {
        let fields = current
            .as_object_mut()
            .ok_or_else(|| ApiError::new(500, "bot.json is not valid"))?;
        for key in FIELDS {
            fields.remove(*key);
            if let Some(v) = target.get(*key) {
                fields.insert((*key).into(), v.clone());
            }
        }
        crate::storage::write_json(profile, &current).await?;
    }
    for key in FIELDS {
        local.remove(*key);
    }
    local.remove("_hbotsRestore");
    local.remove("_hbotsOverlayVersion");
    crate::storage::write_json(&path(profile), &Value::Object(local)).await?;
    Ok(current)
}

fn bindings(value: &Value) -> ApiResult<Map<String, Value>> {
    let fields = value
        .as_object()
        .ok_or_else(|| ApiError::new(500, "bot.json is not valid"))?;
    Ok(FIELDS
        .iter()
        .filter_map(|key| fields.get(*key).map(|v| ((*key).into(), v.clone())))
        .collect())
}

pub(super) async fn save(profile: &Path, value: &Value) -> ApiResult<()> {
    let mut shared = value.clone();
    let fields = shared
        .as_object_mut()
        .ok_or_else(|| ApiError::new(500, "bot.json is not valid"))?;
    let mut local = read(profile).await?;
    local.insert("_hbotsOverlayVersion".into(), Value::from(1));
    local.remove("_hbotsRestore");
    for key in FIELDS {
        if let Some(v) = fields.remove(*key) {
            local.insert((*key).into(), v);
        } else {
            local.remove(*key);
        }
    }
    // Local first: an interrupted migration can be retried without losing bindings.
    crate::storage::write_json(&path(profile), &Value::Object(local)).await?;
    crate::storage::write_json(profile, &shared).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn migrates_and_merges_bindings_and_preserves_unknown_local_fields() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("bot.json");
        let legacy = json!({"name":"Ada", "projectId":"p1", "mainThreadId":"t1"});
        crate::storage::write_json(&profile, &legacy).await.unwrap();
        assert_eq!(load(&profile, legacy).await.unwrap()["mainThreadId"], "t1");
        let shared: Value =
            serde_json::from_slice(&tokio::fs::read(&profile).await.unwrap()).unwrap();
        assert!(shared.get("mainThreadId").is_none());
        assert_eq!(
            load(&profile, shared.clone()).await.unwrap()["projectId"],
            "p1"
        );
        let restored = restore(&profile, shared).await.unwrap();
        assert_eq!(restored["mainThreadId"], "t1");
        let disk: Value =
            serde_json::from_slice(&tokio::fs::read(&profile).await.unwrap()).unwrap();
        assert_eq!(disk["projectId"], "p1");
        assert!(read(&profile).await.unwrap().get("mainThreadId").is_none());
    }

    #[tokio::test]
    async fn bot_list_migrates_in_anytype_mode_and_restores_in_filesystem_mode() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("ada/.bot");
        tokio::fs::create_dir_all(&folder).await.unwrap();
        let profile = folder.join("bot.json");
        crate::storage::write_json(&profile, &json!({"name":"Ada", "mainThreadId":"thread"}))
            .await
            .unwrap();
        let synced = crate::bots::Bots::new(dir.path().to_owned(), String::new(), String::new())
            .with_local_overlay();
        assert_eq!(synced.list().await.unwrap()[0]["mainThreadId"], "thread");
        let shared: Value =
            serde_json::from_slice(&tokio::fs::read(&profile).await.unwrap()).unwrap();
        assert!(shared.get("mainThreadId").is_none());
        let filesystem =
            crate::bots::Bots::new(dir.path().to_owned(), String::new(), String::new());
        assert_eq!(
            filesystem.list().await.unwrap()[0]["mainThreadId"],
            "thread"
        );
        let restored: Value =
            serde_json::from_slice(&tokio::fs::read(&profile).await.unwrap()).unwrap();
        assert_eq!(restored["mainThreadId"], "thread");
    }

    #[tokio::test]
    async fn refuses_unknown_overlay_version_without_changing_files() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("bot.json");
        let shared = json!({"name":"Ada", "mainThreadId":"old"});
        let future = json!({"_hbotsOverlayVersion":2,"mainThreadId":"new"});
        crate::storage::write_json(&profile, &shared).await.unwrap();
        crate::storage::write_json(&path(&profile), &future)
            .await
            .unwrap();
        assert!(load(&profile, shared.clone()).await.is_err());
        assert!(restore(&profile, shared.clone()).await.is_err());
        assert_eq!(
            serde_json::from_slice::<Value>(&tokio::fs::read(&profile).await.unwrap()).unwrap(),
            shared
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&tokio::fs::read(path(&profile)).await.unwrap())
                .unwrap(),
            future
        );
    }

    #[tokio::test]
    async fn interrupted_restore_preserves_later_legacy_binding_changes() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("bot.json");
        let edited = json!({"name":"Ada", "mainThreadId":"legacy-edit"});
        let local = json!({"_hbotsOverlayVersion":1, "mainThreadId":"local", "_hbotsRestore": {
            "base": {}, "target": {"mainThreadId":"local"}
        }});
        crate::storage::write_json(&profile, &edited).await.unwrap();
        crate::storage::write_json(&path(&profile), &local)
            .await
            .unwrap();
        assert_eq!(
            restore(&profile, edited.clone())
                .await
                .unwrap_err()
                .status
                .as_u16(),
            409
        );
        assert_eq!(
            load(&profile, edited.clone())
                .await
                .unwrap_err()
                .status
                .as_u16(),
            409
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&tokio::fs::read(&profile).await.unwrap()).unwrap(),
            edited
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&tokio::fs::read(path(&profile)).await.unwrap())
                .unwrap(),
            local
        );
    }

    #[tokio::test]
    async fn interrupted_restore_finishes_when_target_was_already_written() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("bot.json");
        let shared = json!({"name":"Ada", "mainThreadId":"local"});
        let local = json!({"_hbotsOverlayVersion":1, "mainThreadId":"local", "_hbotsRestore": {
            "base": {}, "target": {"mainThreadId":"local"}
        }});
        crate::storage::write_json(&profile, &shared).await.unwrap();
        crate::storage::write_json(&path(&profile), &local)
            .await
            .unwrap();
        assert_eq!(restore(&profile, shared.clone()).await.unwrap(), shared);
        assert!(read(&profile).await.unwrap().get("_hbotsRestore").is_none());
    }
}
