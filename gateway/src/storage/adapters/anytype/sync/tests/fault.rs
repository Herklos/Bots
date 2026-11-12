use super::super::hook::Step;
use super::support::{files_with_path, remote_body, seed, Kill, World};

#[tokio::test]
async fn kill_before_the_remote_call_then_one_object() {
    kill_push(Step::BeforeRemoteCall).await;
}

#[tokio::test]
async fn kill_after_the_remote_call_then_one_object() {
    kill_push(Step::AfterRemoteCall).await;
}

#[tokio::test]
async fn kill_before_the_manifest_write_then_one_object() {
    kill_push(Step::BeforeManifestWrite).await;
}

#[tokio::test]
async fn kill_a_pull_before_the_manifest_write_then_one_object() {
    let mut world = World::start("dev-fault-pull").await;
    world.live().await;
    let path = "ada/memory/pulled.md";
    let bytes = b"kept\n";
    seed(&world.mock.url, path, bytes, "seed-fault-pull").await;
    world.rig.engine.set_hook(Box::new(Kill {
        step: Step::BeforeManifestWrite,
        left: 1,
    }));
    let cycle = world.rig.engine.run_once().await.unwrap();
    assert!(cycle.stopped, "{cycle:?}");
    world.rig.reopen();
    world.rig.until_quiet().await;
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(world.read(path).unwrap(), bytes);
    assert_eq!(remote_body(&world.mock.url, &rows[0]).await, bytes);
}

async fn kill_push(step: Step) {
    let mut world = World::start("dev-fault").await;
    world.live().await;
    let path = "ada/memory/MEMORY.md";
    let bytes = b"kept";
    world.write(path, bytes);
    world.rig.engine.set_hook(Box::new(Kill { step, left: 1 }));
    let cycle = world.rig.engine.run_once().await.unwrap();
    assert!(cycle.stopped, "{step:?} {cycle:?}");
    world.rig.reopen();
    world.rig.until_quiet().await;
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(rows.len(), 1, "{step:?}");
    assert_eq!(world.read(path).unwrap(), bytes);
    assert_eq!(remote_body(&world.mock.url, &rows[0]).await, bytes);
}
