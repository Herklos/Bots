use super::support::{files_with_path, peer_text, remote_body, World};
use serde_json::json;

#[tokio::test]
async fn local_edit_wins_over_a_remote_tombstone() {
    let mut world = World::start("dev-edit-push").await;
    world.live().await;
    let path = "ada/memory/note.md";
    world.write(path, b"base\n");
    world.rig.until_quiet().await;
    let id = files_with_path(&world.mock.url, path).await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(world.mock.peer_write(
        &id,
        json!({"hb_deleted": true, "hb_content": "", "hb_sha": ""}),
        false,
    ));
    world.write(path, b"kept\n");
    world.rig.until_quiet().await;
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["properties"]["hb_deleted"], false);
    assert_eq!(remote_body(&world.mock.url, &rows[0]).await, b"kept\n");
}

#[tokio::test]
async fn remote_edit_wins_over_a_local_delete() {
    let mut world = World::start("dev-edit-pull").await;
    world.live().await;
    let path = "ada/memory/note.md";
    world.write(path, b"base\n");
    world.rig.until_quiet().await;
    let id = files_with_path(&world.mock.url, path).await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    peer_text(&world.mock, &id, b"remote-edit\n");
    std::fs::remove_file(world.rig.root.path().join(path)).unwrap();
    world.rig.until_quiet().await;
    assert_eq!(world.read(path).unwrap(), b"remote-edit\n");
}
