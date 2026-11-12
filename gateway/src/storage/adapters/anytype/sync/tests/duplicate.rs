use super::support::{files_with_path, remote_body, seed, World};

#[tokio::test]
async fn duplicate_hb_path_keeps_the_earliest_object() {
    let mut world = World::start("dev-dup").await;
    world.live().await;
    let path = "ada/memory/dup.md";
    seed(&world.mock.url, path, b"earliest\n", "dup-a").await;
    seed(&world.mock.url, path, b"later\n", "dup-b").await;
    assert_eq!(world.mock.object_count(), 2);
    world.rig.until_quiet().await;
    assert_eq!(world.read(path).unwrap(), b"earliest\n");
    assert_eq!(world.mock.object_count(), 2);
    assert!(world.rig.engine.status().duplicates >= 1);
    let rows = files_with_path(&world.mock.url, path).await;
    let mut bodies = Vec::new();
    for row in &rows {
        bodies.push(remote_body(&world.mock.url, row).await);
    }
    assert!(bodies.iter().any(|body| body == b"earliest\n"));
    assert!(bodies.iter().any(|body| body == b"later\n"));
}
