use super::super::history;
use super::support::{files_with_path, tombstone_flag, World};
use serde_json::json;

#[tokio::test]
async fn local_delete_tombstones_the_object() {
    let mut world = World::start("dev-del").await;
    world.live().await;
    let path = "ada/memory/note.md";
    world.write(path, b"gone\n");
    world.rig.until_quiet().await;
    std::fs::remove_file(world.rig.root.path().join(path)).unwrap();
    world.rig.engine.run_once().await.unwrap();
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["properties"]["m_deleted"], true);
    assert!(tombstone_flag(world.rig.sync_dir.path(), path));
    assert!(world.read(path).is_none());
}

#[tokio::test]
async fn remote_delete_removes_the_local_file_and_keeps_history() {
    let mut world = World::start("dev-del-remote").await;
    world.live().await;
    let path = "ada/memory/note.md";
    world.write(path, b"gone\n");
    world.rig.until_quiet().await;
    let id = files_with_path(&world.mock.url, path).await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(world.mock.peer_write(
        &id,
        json!({"m_deleted": true, "m_content": "", "m_sha": ""}),
        false,
    ));
    world.rig.until_quiet().await;
    assert!(world.read(path).is_none());
    let history = world.rig.sync_dir.path().join("history");
    assert!(!history::revisions(&history, path).unwrap().is_empty());
}
