use super::support::{api, files_with_path, remote_body, World};

#[tokio::test]
async fn an_archived_object_is_restored_and_takes_the_local_edit() {
    let mut world = World::start("dev-arch").await;
    world.live().await;
    let path = "ada/memory/note.md";
    world.write(path, b"base\n");
    world.rig.until_quiet().await;
    let id = files_with_path(&world.mock.url, path).await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    world.write(path, b"edited\n");
    api(&world.mock.url)
        .archive_object(&id, "archive-1")
        .await
        .unwrap();
    world.rig.until_quiet().await;
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(remote_body(&world.mock.url, &rows[0]).await, b"edited\n");
    assert_eq!(world.read(path).unwrap(), b"edited\n");
}
