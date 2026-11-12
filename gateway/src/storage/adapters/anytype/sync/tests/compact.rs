use super::support::World;
use serde_json::Value;

#[tokio::test]
async fn an_orphan_manifest_entry_is_compacted_away() {
    let mut world = World::start("dev-compact").await;
    world.live().await;
    let path = "ada/memory/kept.md";
    world.write(path, b"kept\n");
    world.rig.until_quiet().await;
    let manifest = world.rig.sync_dir.path().join("manifest.json");
    let mut value: Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    value["entries"]["ada/memory/ghost.md"] = serde_json::json!({
        "tombstone": false,
    });
    std::fs::write(&manifest, format!("{value}\n")).unwrap();
    world.rig.reopen();
    world.rig.until_quiet().await;
    let value: Value = serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    assert!(value["entries"].get("ada/memory/ghost.md").is_none());
    assert!(value["entries"].get(path).is_some());
    assert_eq!(world.read(path).unwrap(), b"kept\n");
}
