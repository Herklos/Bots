use super::support::{assert_roundtrip, World};

#[tokio::test]
async fn push_create_uploads_a_new_file() {
    let mut world = World::start("dev-push").await;
    world.live().await;
    let bytes = b"created\n";
    world.write("ada/memory/note.md", bytes);
    world.rig.until_quiet().await;
    assert_roundtrip(&world, "ada/memory/note.md", bytes).await;
    assert_eq!(world.mock.object_count(), 1);
}
