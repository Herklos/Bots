use super::super::hook::Step;
use super::super::outbox::{self, Op};
use super::super::sha256_hex;
use super::support::{files_with_path, remote_body, Kill, World};
use serde_json::Value;

#[tokio::test]
async fn a_killed_create_replays_the_same_outbox_key() {
    let mut world = World::start("dev-out").await;
    world.live().await;
    let path = "ada/memory/note.md";
    let bytes = b"kept\n";
    world.write(path, bytes);
    world.rig.engine.set_hook(Box::new(Kill {
        step: Step::BeforeRemoteCall,
        left: 1,
    }));
    let cycle = world.rig.engine.run_once().await.unwrap();
    assert!(cycle.stopped, "{cycle:?}");
    assert_eq!(world.mock.object_count(), 0);
    let key = outbox::idem_key(&world.rig.device, path, Op::Create, &sha256_hex(bytes));
    let text = std::fs::read_to_string(world.rig.sync_dir.path().join("outbox.jsonl")).unwrap();
    let line: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    assert_eq!(line["idem_key"], key);
    world.rig.reopen();
    world.rig.until_quiet().await;
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(remote_body(&world.mock.url, &rows[0]).await, bytes);
}
