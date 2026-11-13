use super::support::{api, files_with_path, peer_text, World};
use serde_json::json;

#[tokio::test]
async fn a_conflict_resolved_in_anytype_clears_the_local_entry() {
    let mut world = World::start("dev-remote-resolve").await;
    world.live().await;
    let path = "ada/memory/note.md";
    world.write(path, b"alpha\nmiddle\nomega\n");
    world.rig.until_quiet().await;
    let id = files_with_path(&world.mock.url, path).await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    world.write(path, b"alpha\nLOCAL\nomega\n");
    peer_text(&world.mock, &id, b"alpha\nREMOTE\nomega\n");
    world.rig.until_quiet().await;
    assert!(world.rig.engine.conflict_for(path).is_some());
    let sidecar = sidecars(world.rig.root.path());
    assert_eq!(sidecar.len(), 1);

    peer_text(&world.mock, &id, b"alpha\nLOCAL\nomega\n");
    let conflicts = api(&world.mock.url)
        .search(&json!({
            "type": "hb_conflict",
            "fields": ["hb_path", "hb_conflict_state"],
        }))
        .await
        .unwrap();
    let conflict_id = conflicts
        .iter()
        .find(|row| row["properties"]["hb_path"].as_str() == Some(path))
        .and_then(|row| row["id"].as_str())
        .unwrap()
        .to_string();
    assert!(world.mock.peer_write(
        &conflict_id,
        json!({"hb_conflict_state": "resolved"}),
        false,
    ));
    world.rig.until_quiet().await;
    assert!(world.rig.engine.conflict_for(path).is_none());
    assert!(sidecars(world.rig.root.path()).is_empty());
    assert_eq!(world.read(path).unwrap(), b"alpha\nLOCAL\nomega\n");
    let _ = sidecar;
}

#[tokio::test]
async fn a_conflict_dismissed_in_anytype_clears_the_local_entry() {
    let mut world = World::start("dev-remote-dismiss").await;
    world.live().await;
    let path = "ada/memory/note.md";
    world.write(path, b"one\n");
    world.rig.until_quiet().await;
    let id = files_with_path(&world.mock.url, path).await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    world.write(path, b"local\n");
    peer_text(&world.mock, &id, b"remote\n");
    world.rig.until_quiet().await;
    assert!(world.rig.engine.conflict_for(path).is_some());
    peer_text(&world.mock, &id, b"local\n");
    let conflicts = api(&world.mock.url)
        .search(&json!({
            "type": "hb_conflict",
            "fields": ["hb_path", "hb_conflict_state"],
        }))
        .await
        .unwrap();
    let conflict_id = conflicts[0]["id"].as_str().unwrap();
    assert!(world.mock.peer_write(
        conflict_id,
        json!({"hb_conflict_state": "dismissed"}),
        false,
    ));
    world.rig.until_quiet().await;
    assert!(world.rig.engine.conflict_for(path).is_none());
    assert!(sidecars(world.rig.root.path()).is_empty());
}

fn sidecars(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    walk(root, &mut found);
    found
}

fn walk(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, found);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains(".conflict-"))
        {
            found.push(path);
        }
    }
}
