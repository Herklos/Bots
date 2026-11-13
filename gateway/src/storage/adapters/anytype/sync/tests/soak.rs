//! A simulated day against the mock: ten Bots, two thousand files, edits, and
//! process kills. The two trees are the folder and the mock, not a live space.

use super::super::hook::{Control, Hook, Step};
use super::support::{remote_body, World};
use serde_json::Value;
use std::collections::BTreeMap;

struct Kill {
    left: u8,
}

impl Hook for Kill {
    fn on_step(&mut self, step: Step, _path: &str) -> Control {
        if step != Step::BeforeRemoteCall || self.left == 0 {
            return Control::Continue;
        }
        self.left -= 1;
        if self.left == 0 {
            Control::Stop
        } else {
            Control::Continue
        }
    }
}

#[tokio::test]
async fn ten_bots_and_two_thousand_files_survive_a_day_of_edits() {
    let mut world = World::start("dev-soak").await;
    world.rig.config.max_writes = 500;
    world.rig.config.max_passes = 12;
    world.rig.config.hash_budget = std::time::Duration::from_secs(30);
    world.rig.reopen();

    let mut expected = BTreeMap::new();
    for bot in 0..10 {
        for file in 0..200 {
            let rel = format!("bot{bot}/memory/n{file}.md");
            let text = format!("bot {bot} file {file}\n");
            world.write(&rel, text.as_bytes());
            expected.insert(rel, text);
        }
    }
    world.live().await;
    assert_eq!(expected.len(), 2_000);

    let mut state = 0xdecaf_u64;
    for step in 0..80 {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let bot = state % 10;
        let file = (state / 10) % 200;
        let rel = format!("bot{bot}/memory/n{file}.md");
        if step % 17 == 0 {
            std::fs::remove_file(world.rig.root.path().join(&rel)).unwrap();
            expected.remove(&rel);
        } else {
            let text = format!("edit {step} of {rel}\n");
            world.write(&rel, text.as_bytes());
            expected.insert(rel, text);
        }
        if step % 11 == 10 {
            world.rig.engine.set_hook(Box::new(Kill { left: 1 }));
            let cycle = world.rig.engine.run_once().await.unwrap();
            assert!(cycle.stopped || cycle.applied > 0 || cycle.deferred);
            world.rig.reopen();
        }
    }
    for _ in 0..40 {
        let cycle = world.rig.engine.run_once().await.unwrap();
        assert!(!cycle.stopped, "{cycle:?}");
        if cycle.applied == 0 && !cycle.deferred && cycle.status.retry_in.is_zero() {
            break;
        }
        if cycle.status.retry_in > std::time::Duration::ZERO {
            tokio::time::sleep(cycle.status.retry_in).await;
        }
    }
    assert_eq!(world.rig.engine.status().conflicts, 0);
    assert_eq!(world.rig.engine.status().outbox, 0);
    let remote = remote_tree(&world.mock.url).await;
    assert_eq!(remote.len(), expected.len());
    for (rel, text) in &expected {
        assert_eq!(
            world.read(rel).as_deref(),
            Some(text.as_bytes()),
            "{rel} local"
        );
        assert_eq!(
            remote.get(rel).map(Vec::as_slice),
            Some(text.as_bytes()),
            "{rel} remote"
        );
    }
}

async fn remote_tree(url: &str) -> BTreeMap<String, Vec<u8>> {
    let rows = super::support::m_files(url).await;
    let mut tree = BTreeMap::new();
    for row in rows {
        if row["properties"]["m_deleted"] == Value::Bool(true) {
            continue;
        }
        let Some(path) = row["properties"]["m_path"].as_str() else {
            continue;
        };
        let body = remote_body(url, &row).await;
        tree.insert(path.to_string(), body);
    }
    tree
}
