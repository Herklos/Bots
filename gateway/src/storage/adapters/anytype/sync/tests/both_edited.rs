use super::super::apply::Keep;
use super::support::{files_with_path, peer_text, remote_body, World};
use std::path::Path;

#[tokio::test]
async fn both_edited_text_merges_cleanly() {
    let mut world = World::start("dev-merge").await;
    world.live().await;
    let path = "ada/memory/note.md";
    world.write(path, b"alpha\nmiddle\nomega\n");
    world.rig.until_quiet().await;
    let id = files_with_path(&world.mock.url, path).await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    world.write(path, b"ALPHA\nmiddle\nomega\n");
    peer_text(&world.mock, &id, b"alpha\nmiddle\nOMEGA\n");
    world.rig.until_quiet().await;
    let local = world.read(path).unwrap();
    let text = String::from_utf8(local.clone()).unwrap();
    assert!(text.contains("ALPHA"), "{text}");
    assert!(text.contains("OMEGA"), "{text}");
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(remote_body(&world.mock.url, &rows[0]).await, local);
    assert!(world.rig.engine.conflict_for(path).is_none());
}

#[tokio::test]
async fn both_edited_conflict_keeps_local_until_resolved() {
    let mut world = World::start("dev-conflict").await;
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
    assert_eq!(world.read(path).unwrap(), b"alpha\nLOCAL\nomega\n");
    assert!(!sidecars(world.rig.root.path()).is_empty());
    let conflict = world.rig.engine.conflict_for(path).unwrap();
    world
        .rig
        .engine
        .resolve(&conflict, Keep::Local)
        .await
        .unwrap();
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(
        remote_body(&world.mock.url, &rows[0]).await,
        b"alpha\nLOCAL\nomega\n"
    );
    assert!(sidecars(world.rig.root.path()).is_empty());
}

#[tokio::test]
async fn both_edited_whole_file_conflicts() {
    let mut world = World::start("dev-whole").await;
    world.live().await;
    let path = "ada/.bot/library.json";
    world.write(path, b"{\"v\":1}\n");
    world.rig.until_quiet().await;
    let id = files_with_path(&world.mock.url, path).await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    world.write(path, b"{\"v\":2}\n");
    peer_text(&world.mock, &id, b"{\"v\":3}\n");
    world.rig.until_quiet().await;
    assert_eq!(world.read(path).unwrap(), b"{\"v\":2}\n");
    assert!(world.rig.engine.conflict_for(path).is_some());
}

fn sidecars(root: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    walk(root, &mut found);
    found
}

fn walk(dir: &Path, found: &mut Vec<std::path::PathBuf>) {
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
