use super::super::status::SyncState;
use super::support::{files_with_path, remote_body, World};
use crate::storage::adapters::anytype::tests::mock::Mock;
use std::net::SocketAddr;
use std::time::Duration;

#[tokio::test]
async fn offline_keeps_the_edit_and_reconnect_pushes_it() {
    let mut world = World::start("dev-off").await;
    world.live().await;
    let path = "ada/memory/note.md";
    world.write(path, b"v1\n");
    world.rig.until_quiet().await;
    world.write(path, b"v2\n");
    world.mock.shutdown().await;
    let cycle = world.rig.engine.run_once().await.unwrap();
    assert_eq!(cycle.status.state, SyncState::Offline, "{cycle:?}");
    assert!(world.rig.engine.status().outbox > 0);
    assert_eq!(world.read(path).unwrap(), b"v2\n");
    rebind(&mut world.mock).await;
    world.rig.reopen();
    world.rig.until_quiet().await;
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(remote_body(&world.mock.url, &rows[0]).await, b"v2\n");
    assert_eq!(world.rig.engine.status().outbox, 0);
}

async fn rebind(mock: &mut Mock) {
    let addr: SocketAddr = mock.url.trim_start_matches("http://").parse().unwrap();
    mock.shutdown().await;
    let mut last = String::new();
    for _ in 0..40 {
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                *mock = Mock::from_listener(listener);
                super::support::prepare(mock).await;
                return;
            }
            Err(error) => {
                last = error.to_string();
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
    }
    panic!("rebind {addr}: {last}");
}
