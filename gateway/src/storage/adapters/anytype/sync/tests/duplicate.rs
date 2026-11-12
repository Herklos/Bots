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
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(rows.len(), 1, "the later object is archived");
    assert_eq!(remote_body(&world.mock.url, &rows[0]).await, b"earliest\n");
    assert_eq!(world.rig.engine.status().duplicates, 0);
}
