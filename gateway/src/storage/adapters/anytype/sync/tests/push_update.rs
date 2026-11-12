use super::support::{files_with_path, remote_body, World};

#[tokio::test]
async fn push_update_keeps_the_object_id() {
    let mut world = World::start("dev-update").await;
    world.live().await;
    let path = "ada/memory/note.md";
    world.write(path, b"v1\n");
    world.rig.until_quiet().await;
    let id = files_with_path(&world.mock.url, path).await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    world.write(path, b"v2\n");
    world.rig.until_quiet().await;
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], id);
    assert_eq!(remote_body(&world.mock.url, &rows[0]).await, b"v2\n");
    assert_eq!(world.read(path).unwrap(), b"v2\n");
}
