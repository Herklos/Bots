use super::super::hook::{Control, Hook, Step};
use super::super::status::Phase;
use super::support::{files_with_path, hb_files, seed, write_rel, World};
use std::sync::{Arc, Mutex};

struct Order {
    paths: Arc<Mutex<Vec<String>>>,
}

impl Hook for Order {
    fn on_step(&mut self, step: Step, path: &str) -> Control {
        if step == Step::BeforeManifestWrite {
            self.paths.lock().expect("order").push(path.to_string());
        }
        Control::Continue
    }
}

#[tokio::test]
async fn first_sync_dry_run_then_import_overlapping_paths() {
    let mut world = World::start("dev-first").await;
    let root = world.rig.root.path().to_path_buf();
    write_rel(&root, ".botapp/shared-memory.md", b"shared\n");
    write_rel(&root, "ada/memory/local.md", b"only-local\n");
    write_rel(&root, "ada/skills/skill.md", b"skill\n");
    write_rel(&root, "ada/memory/both.md", b"from-disk\n");
    seed(
        &world.mock.url,
        "ada/memory/remote.md",
        b"from-anytype\n",
        "seed-remote",
    )
    .await;
    seed(
        &world.mock.url,
        "ada/memory/both.md",
        b"from-anytype\n",
        "seed-both",
    )
    .await;
    let before = hb_files(&world.mock.url).await.len();
    let preview = world.rig.engine.dry_run().await.unwrap();
    assert_eq!(preview.push, 3);
    assert_eq!(preview.pull, 1);
    assert_eq!(preview.conflict, 1);
    assert!(preview.estimate > std::time::Duration::ZERO);
    world.rig.engine.run_once().await.unwrap();
    assert_eq!(world.rig.engine.phase(), Phase::NeedsImport);
    assert_eq!(hb_files(&world.mock.url).await.len(), before);
    assert!(world.read("ada/memory/remote.md").is_none());
    assert_eq!(world.read("ada/memory/both.md").unwrap(), b"from-disk\n");

    let paths = Arc::new(Mutex::new(Vec::new()));
    world.rig.engine.set_hook(Box::new(Order {
        paths: Arc::clone(&paths),
    }));
    world.rig.engine.confirm_import().await.unwrap();
    assert_eq!(world.rig.engine.phase(), Phase::Live);
    assert_eq!(world.read("ada/memory/both.md").unwrap(), b"from-disk\n");
    assert_eq!(
        world.read("ada/memory/remote.md").unwrap(),
        b"from-anytype\n"
    );
    assert_eq!(
        files_with_path(&world.mock.url, ".botapp/shared-memory.md")
            .await
            .len(),
        1
    );
    assert_eq!(hb_files(&world.mock.url).await.len(), before + 3);
    let seen = paths.lock().expect("order").clone();
    let shared = seen
        .iter()
        .position(|path| path == ".botapp/shared-memory.md")
        .unwrap();
    let memory = seen
        .iter()
        .position(|path| path == "ada/memory/local.md")
        .unwrap();
    let skill = seen
        .iter()
        .position(|path| path == "ada/skills/skill.md")
        .unwrap();
    assert!(shared < memory && memory < skill, "{seen:?}");
}
