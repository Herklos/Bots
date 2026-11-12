use super::support::{files_with_path, peer_text, World};

#[tokio::test]
async fn pull_update_replaces_the_local_file() {
    let mut world = World::start("dev-pull-up").await;
    world.live().await;
    let path = "ada/memory/note.md";
    world.write(path, b"v1\n");
    world.rig.until_quiet().await;
    let id = files_with_path(&world.mock.url, path).await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    peer_text(&world.mock, &id, b"v2\n");
    world.rig.until_quiet().await;
    assert_eq!(world.read(path).unwrap(), b"v2\n");
}
