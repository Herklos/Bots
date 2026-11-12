use super::support::{files_with_path, remote_body, tombstone_flag, World};

#[tokio::test]
async fn a_tombstone_comes_back_when_the_file_returns() {
    let mut world = World::start("dev-tomb").await;
    world.live().await;
    let path = "ada/memory/note.md";
    world.write(path, b"first\n");
    world.rig.until_quiet().await;
    std::fs::remove_file(world.rig.root.path().join(path)).unwrap();
    world.rig.engine.run_once().await.unwrap();
    assert!(tombstone_flag(world.rig.sync_dir.path(), path));
    assert_eq!(
        files_with_path(&world.mock.url, path).await[0]["properties"]["hb_deleted"],
        true
    );
    world.write(path, b"again\n");
    world.rig.engine.run_once().await.unwrap();
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["properties"]["hb_deleted"], false);
    assert_eq!(remote_body(&world.mock.url, &rows[0]).await, b"again\n");
    assert!(!tombstone_flag(world.rig.sync_dir.path(), path));
}
