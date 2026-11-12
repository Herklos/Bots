use super::super::hook::{Control, Hook, Step};
use super::super::status::SyncState;
use super::support::{files_with_path, remote_body, World};
use crate::storage::adapters::anytype::tests::mock::Mock;

struct Limit {
    mock: Mock,
    armed: bool,
}

impl Hook for Limit {
    fn on_step(&mut self, step: Step, _path: &str) -> Control {
        if step == Step::BeforeRemoteCall && !self.armed {
            self.armed = true;
            self.mock.set_rate_limit(1, 0);
        }
        Control::Continue
    }
}

#[tokio::test]
async fn a_429_pauses_then_the_next_cycle_pushes() {
    let mut world = World::start("dev-429").await;
    world.live().await;
    let path = "ada/memory/note.md";
    world.write(path, b"v1\n");
    world.rig.until_quiet().await;
    world.write(path, b"v2\n");
    world.rig.engine.set_hook(Box::new(Limit {
        mock: world.mock.clone(),
        armed: false,
    }));
    let paused = world.rig.engine.run_once().await.unwrap();
    assert!(paused.deferred, "{paused:?}");
    assert!(world.rig.engine.status().outbox > 0);
    assert!(world.rig.engine.write_rate() < 10_000.0);
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(remote_body(&world.mock.url, &rows[0]).await, b"v1\n");
    let early = world.rig.engine.run_once().await.unwrap();
    assert_eq!(early.applied, 0);
    tokio::time::sleep(paused.status.retry_in + std::time::Duration::from_millis(20)).await;
    world.rig.until_quiet().await;
    assert_ne!(world.rig.engine.status().state, SyncState::Offline);
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(remote_body(&world.mock.url, &rows[0]).await, b"v2\n");
    assert_eq!(world.rig.engine.status().outbox, 0);
}
