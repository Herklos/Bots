use super::support::{seed, World};

#[tokio::test]
async fn pull_create_writes_a_remote_file() {
    let mut world = World::start("dev-pull").await;
    world.live().await;
    let bytes = b"from-anytype\n";
    seed(&world.mock.url, "ada/memory/remote.md", bytes, "seed-pull").await;
    world.rig.until_quiet().await;
    assert_eq!(world.read("ada/memory/remote.md").unwrap(), bytes);
    assert_eq!(world.mock.object_count(), 2);
}
