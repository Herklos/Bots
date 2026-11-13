use super::super::hook::{Control, Hook, Step};
use super::support::{files_with_path, remote_body, World};
use crate::storage::adapters::anytype::tests::mock::Mock;
use serde_json::json;

struct Bump {
    mock: Mock,
    id: String,
    armed: bool,
}

impl Hook for Bump {
    fn on_step(&mut self, step: Step, _path: &str) -> Control {
        if step == Step::BeforeRemoteCall && !self.armed {
            self.armed = true;
            assert!(self.mock.peer_write(&self.id, json!({"m_rev": 9}), false));
        }
        Control::Continue
    }
}

#[tokio::test]
async fn an_etag_mismatch_retries_with_the_current_tag() {
    let mut world = World::start("dev-etag").await;
    world.live().await;
    let path = "ada/memory/note.md";
    world.write(path, b"v1\n");
    world.rig.until_quiet().await;
    let id = files_with_path(&world.mock.url, path).await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    world.write(path, b"v2\n");
    world.rig.engine.set_hook(Box::new(Bump {
        mock: world.mock.clone(),
        id,
        armed: false,
    }));
    let cycle = world.rig.engine.run_once().await.unwrap();
    assert!(!cycle.stopped, "{cycle:?}");
    assert_eq!(cycle.applied, 1, "{cycle:?}");
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(remote_body(&world.mock.url, &rows[0]).await, b"v2\n");
}
