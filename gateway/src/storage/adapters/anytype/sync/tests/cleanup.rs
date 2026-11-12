use super::support::{api, files_with_path, remote_body, seed, World};
use serde_json::json;

#[tokio::test]
async fn an_empty_earliest_object_takes_the_duplicate_then_the_extra_is_archived() {
    let mut world = World::start("dev-merge-dup").await;
    let path = "ada/memory/dup.md";
    let client = api(&world.mock.url);
    client
        .create_object(
            &json!({
                "type": "hb_file",
                "name": path,
                "properties": {"hb_path": path, "hb_deleted": false, "hb_sha": ""},
            }),
            "shell",
        )
        .await
        .unwrap();
    seed(&world.mock.url, path, b"later body\n", "dup-body").await;
    world.live().await;
    world.rig.until_quiet().await;
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(
        remote_body(&world.mock.url, &rows[0]).await,
        b"later body\n"
    );
    assert_eq!(world.read(path).unwrap(), b"later body\n");
    assert_eq!(world.rig.engine.status().duplicates, 0);
}
