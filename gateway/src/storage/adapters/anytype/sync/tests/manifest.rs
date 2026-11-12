use super::super::manifest::{
    load, save, Entry, Identity, Loaded, Manifest, ManifestError, VERSION,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn identity() -> Identity {
    Identity {
        host: "http://127.0.0.1:31009".into(),
        space_id: "space".into(),
        root: "/bots".into(),
    }
}

fn sample() -> Manifest {
    let mut entries = BTreeMap::new();
    let mut extra = BTreeMap::new();
    extra.insert("origin".into(), json!("legacy"));
    entries.insert(
        "ada/memory/MEMORY.md".into(),
        Entry {
            base_sha: Some("abc".into()),
            object_id: Some("obj".into()),
            remote_etag: Some("etag".into()),
            remote_modified: Some("2026-11-12T00:00:00Z".into()),
            local_mtime_ns: Some(1_700_000_000_000_000_000),
            local_size: Some(12),
            tombstone: false,
            extra,
        },
    );
    let mut manifest = Manifest::fresh(&identity(), "device-1");
    manifest.entries = entries;
    manifest.extra.insert("note".into(), json!("keep"));
    manifest
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[tokio::test]
async fn save_is_atomic_and_mode_0600() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync/manifest.json");
    let manifest = sample();
    save(&path, &manifest).await.unwrap();
    let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["manifest.json".to_string()]);
    #[cfg(unix)]
    assert_eq!(mode(&path), 0o600);

    let Loaded::Current(loaded) = load(&path, &identity()).unwrap() else {
        panic!("expected the saved manifest");
    };
    assert_eq!(loaded, manifest);
    assert_eq!(loaded.version, VERSION);
    assert_eq!(loaded.extra.get("note"), Some(&json!("keep")));
    assert_eq!(
        loaded.entries["ada/memory/MEMORY.md"].extra.get("origin"),
        Some(&json!("legacy"))
    );
}

#[tokio::test]
async fn host_change_drops_entries_and_keeps_the_device() {
    assert_reset(|id| id.host = "http://other".into()).await;
}

#[tokio::test]
async fn space_change_drops_entries_and_keeps_the_device() {
    assert_reset(|id| id.space_id = "other-space".into()).await;
}

#[tokio::test]
async fn root_change_drops_entries_and_keeps_the_device() {
    assert_reset(|id| id.root = "/other".into()).await;
}

async fn assert_reset(change: fn(&mut Identity)) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("manifest.json");
    save(&path, &sample()).await.unwrap();
    let before = std::fs::read(&path).unwrap();
    let mut changed = identity();
    change(&mut changed);
    let Loaded::Reset(manifest) = load(&path, &changed).unwrap() else {
        panic!("expected a reset");
    };
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(manifest.entries.is_empty());
    assert_eq!(manifest.device_id, "device-1");
    assert_eq!(manifest.host, changed.host);
    assert_eq!(manifest.space_id, changed.space_id);
    assert_eq!(manifest.root, changed.root);
    assert_eq!(manifest.version, VERSION);
    save(&path, &manifest).await.unwrap();
    let Loaded::Current(again) = load(&path, &changed).unwrap() else {
        panic!("the reset manifest is current for the new identity");
    };
    assert!(again.entries.is_empty());
}

#[tokio::test]
async fn missing_version_migrates_on_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("manifest.json");
    let id = identity();
    std::fs::write(
        &path,
        format!(
            r#"{{"host":"{}","space_id":"{}","root":"{}","device_id":"device-1","entries":{{"ada/memory/a.md":{{"base_sha":"abc"}}}}}}"#,
            id.host, id.space_id, id.root
        ),
    )
    .unwrap();
    let Loaded::Current(manifest) = load(&path, &id).unwrap() else {
        panic!("version 0 should load");
    };
    assert_eq!(manifest.version, VERSION);
    assert_eq!(
        manifest.entries["ada/memory/a.md"].base_sha.as_deref(),
        Some("abc")
    );
    assert!(!std::fs::read_to_string(&path)
        .unwrap()
        .contains("\"version\""));
    save(&path, &manifest).await.unwrap();
    let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(value["version"], json!(VERSION));
    assert_eq!(
        value["entries"]["ada/memory/a.md"]["base_sha"],
        json!("abc")
    );
}

#[test]
fn a_future_version_is_not_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("manifest.json");
    let raw = "{\"version\":2,\"unreadable\":true}\n";
    std::fs::write(&path, raw).unwrap();
    let error = load(&path, &identity()).unwrap_err();
    assert!(matches!(error, ManifestError::FutureVersion(2)));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), raw);
}

#[test]
fn a_corrupt_file_is_not_treated_as_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("manifest.json");
    std::fs::write(&path, "{").unwrap();
    assert!(matches!(
        load(&path, &identity()),
        Err(ManifestError::Corrupt(_))
    ));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{");
}

#[test]
fn a_missing_file_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync/manifest.json");
    assert_eq!(load(&path, &identity()).unwrap(), Loaded::Missing);
    assert!(!path.exists());
}
