//! The watch store: corruption recovery, concurrent edits, and mode 600.

use super::super::*;

fn store_ready() -> (tempfile::TempDir, WatchFile) {
    let dir = tempfile::tempdir().unwrap();
    let file = WatchFile::new(&dir.path().join("bots"), "ada");
    std::fs::create_dir_all(file.path.parent().unwrap()).unwrap();
    (dir, file)
}

#[tokio::test]
async fn a_missing_store_is_empty_and_a_corrupt_one_recovers() {
    let (_dir, file) = store_ready();
    assert!(file.load().await["watches"].as_array().unwrap().is_empty());
    std::fs::write(&file.path, b"{not json").unwrap();
    assert_eq!(
        file.load().await["watches"],
        json!([]),
        "a corrupt file recovers to empty"
    );
}

#[tokio::test]
async fn concurrent_creates_and_a_delete_each_keep_the_file_whole() {
    let (_dir, file) = store_ready();
    let mut makers = Vec::new();
    for i in 0..4 {
        let file = file.clone();
        makers.push(tokio::spawn(async move {
            file.update(|watches| {
                watches.push(json!({ "id": format!("w_a{i}"), "name": "a", "enabled": true }));
                Ok(())
            })
            .await
        }));
    }
    for maker in makers {
        assert!(maker.await.unwrap().is_ok());
    }
    assert_eq!(
        file.load().await["watches"].as_array().map(Vec::len),
        Some(4)
    );
    file.update(|watches| {
        watches.retain(|w| w["id"] != json!("w_a2"));
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(
        file.load().await["watches"].as_array().map(Vec::len),
        Some(3)
    );
}

#[tokio::test]
async fn the_store_is_written_mode_600() {
    let (_dir, file) = store_ready();
    file.save(&json!({ "version": 1, "watches": [] }))
        .await
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file.path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "watches.json is owner-only");
    }
}
