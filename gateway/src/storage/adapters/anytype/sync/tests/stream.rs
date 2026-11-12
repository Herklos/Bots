//! The live feed against the mock. The driver cycles only when a stream note says to,
//! so a passing test is the stream (or the immediate poll on `Down`), not the 3s timer.

use super::super::feed::{self, FeedSettings, Note};
use super::super::status;
use super::support::{api, files_with_path, peer_text, World};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

fn settings(backoff: Duration) -> FeedSettings {
    FeedSettings {
        heartbeat: Duration::from_secs(30),
        idle: Duration::from_secs(30),
        backoff_floor: backoff,
    }
}

struct Watch {
    notes: mpsc::UnboundedReceiver<Note>,
}

async fn open_watch(world: &mut World, backoff: Duration) -> Watch {
    let (tx, notes) = mpsc::unbounded_channel();
    feed::spawn(api(&world.mock.url), settings(backoff), tx);
    let mut watch = Watch { notes };
    watch.until_up(&mut world.rig.engine).await;
    watch
}

impl Watch {
    async fn until_up(&mut self, engine: &mut super::super::engine::Engine) {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut saw_up = false;
        let mut saw_snapshot = false;
        let mut stream_up = false;
        while !saw_up || !saw_snapshot {
            let note = self.recv_until(deadline).await;
            match &note {
                Note::Up => saw_up = true,
                Note::Snapshot(_) => saw_snapshot = true,
                _ => {}
            }
            let _ = feed::ingest(engine, note, &mut stream_up);
        }
        assert!(stream_up);
    }

    async fn recv_until(&mut self, deadline: Instant) -> Note {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for a stream note"
        );
        let left = deadline.saturating_duration_since(Instant::now());
        tokio::time::timeout(left, self.notes.recv())
            .await
            .expect("timed out waiting for a stream note")
            .expect("search stream closed")
    }
}

async fn synced(device: &str) -> (World, String) {
    let mut world = World::start(device).await;
    world.live().await;
    let path = "ada/memory/note.md".to_string();
    world.write(&path, b"v1\n");
    world.rig.until_quiet().await;
    (world, path)
}

async fn object_id(world: &World, path: &str) -> String {
    files_with_path(&world.mock.url, path).await[0]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn a_down_stream_stays_inside_the_ten_second_poll_and_offline_does_not() {
    let scan = status::next_wait(false, Duration::from_secs(30), false);
    assert_eq!(feed::cycle_wait(false, Duration::from_secs(30), true), scan);
    let down = feed::cycle_wait(false, Duration::from_secs(30), false);
    assert!(down <= Duration::from_secs(10));
    assert!(down <= scan);
    let offline = feed::cycle_wait(true, Duration::from_secs(30), false);
    assert!(offline > Duration::from_secs(10));
}

#[tokio::test]
async fn a_remote_edit_lands_from_the_stream_within_a_couple_of_seconds() {
    let (mut world, path) = synced("dev-stream").await;
    let id = object_id(&world, &path).await;
    let mut watch = open_watch(&mut world, Duration::from_secs(30)).await;
    let started = Instant::now();
    peer_text(&world.mock, &id, b"v2\n");
    let mut stream_up = true;
    let deadline = started + Duration::from_secs(2);
    while world.read(&path).as_deref() != Some(&b"v2\n"[..]) {
        let note = watch.recv_until(deadline).await;
        if feed::ingest(&mut world.rig.engine, note, &mut stream_up) {
            world.rig.engine.run_once().await.unwrap();
        }
    }
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(stream_up);
}

#[tokio::test]
async fn a_killed_stream_falls_back_to_polling_without_a_gap() {
    let (mut world, path) = synced("dev-stream-poll").await;
    let id = object_id(&world, &path).await;
    let mut watch = open_watch(&mut world, Duration::from_secs(5)).await;
    world.mock.drop_streams();
    world.mock.set_stream_cap(0);
    peer_text(&world.mock, &id, b"v2\n");
    let started = Instant::now();
    let mut stream_up = true;
    let deadline = started + Duration::from_secs(2);
    while world.read(&path).as_deref() != Some(&b"v2\n"[..]) {
        let note = watch.recv_until(deadline).await;
        if feed::ingest(&mut world.rig.engine, note, &mut stream_up) {
            world.rig.engine.run_once().await.unwrap();
        }
    }
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(!stream_up);
}

#[tokio::test]
async fn dropping_the_stream_mid_run_reconnects_and_applies_the_edit() {
    let (mut world, path) = synced("dev-stream-drop").await;
    let id = object_id(&world, &path).await;
    let mut watch = open_watch(&mut world, Duration::from_millis(250)).await;
    world.mock.drop_streams();
    let mut stream_up = true;
    let down_by = Instant::now() + Duration::from_secs(2);
    while stream_up {
        let note = watch.recv_until(down_by).await;
        let _ = feed::ingest(&mut world.rig.engine, note, &mut stream_up);
    }
    peer_text(&world.mock, &id, b"v2\n");
    let started = Instant::now();
    let deadline = started + Duration::from_secs(2);
    let mut from_reconnect = false;
    while world.read(&path).as_deref() != Some(&b"v2\n"[..]) {
        let note = watch.recv_until(deadline).await;
        let streamed = matches!(note, Note::Snapshot(_) | Note::Changed(_));
        if feed::ingest(&mut world.rig.engine, note, &mut stream_up) && streamed {
            from_reconnect = true;
            world.rig.engine.run_once().await.unwrap();
        }
    }
    assert!(from_reconnect);
    assert!(started.elapsed() < Duration::from_secs(2));
    let back_by = Instant::now() + Duration::from_secs(2);
    while !stream_up {
        let note = watch.recv_until(back_by).await;
        let _ = feed::ingest(&mut world.rig.engine, note, &mut stream_up);
    }
}

#[tokio::test]
async fn an_event_for_the_sha_just_pushed_is_ignored() {
    let (mut world, path) = synced("dev-stream-echo").await;
    let mut watch = open_watch(&mut world, Duration::from_secs(30)).await;
    world.write(&path, b"v2\n");
    world.rig.engine.run_once().await.unwrap();
    assert_eq!(world.read(&path).as_deref(), Some(&b"v2\n"[..]));
    let mut stream_up = true;
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let note = watch.recv_until(deadline).await;
        if matches!(note, Note::Changed(_)) {
            assert!(
                !feed::ingest(&mut world.rig.engine, note, &mut stream_up),
                "the push we just made was treated as a remote edit"
            );
            break;
        }
        let _ = feed::ingest(&mut world.rig.engine, note, &mut stream_up);
    }
}

#[tokio::test]
async fn a_snapshot_that_omits_an_object_still_in_search_does_not_delete_it() {
    let (mut world, path) = synced("dev-stream-omit").await;
    let id = object_id(&world, &path).await;
    let mut watch = open_watch(&mut world, Duration::from_millis(80)).await;
    world.mock.reset_gets();
    world.mock.omit_from_stream(&id);
    world.mock.drop_streams();
    let mut stream_up = true;
    let mut snapshots = 0_u32;
    let deadline = Instant::now() + Duration::from_secs(2);
    while snapshots < 1 {
        let note = watch.recv_until(deadline).await;
        if matches!(note, Note::Snapshot(_)) {
            snapshots += 1;
        }
        if feed::ingest(&mut world.rig.engine, note, &mut stream_up) {
            world.rig.engine.run_once().await.unwrap();
        }
    }
    assert_eq!(world.read(&path).as_deref(), Some(&b"v1\n"[..]));
    assert_eq!(world.mock.get_count(), 0);
}

#[tokio::test]
async fn an_object_missing_from_the_snapshot_is_read_once_before_it_is_deleted() {
    let (mut world, path) = synced("dev-stream-gone").await;
    let id = object_id(&world, &path).await;
    let mut watch = open_watch(&mut world, Duration::from_millis(250)).await;
    world.mock.drop_streams();
    let mut stream_up = true;
    let down_by = Instant::now() + Duration::from_secs(2);
    while stream_up {
        let note = watch.recv_until(down_by).await;
        let _ = feed::ingest(&mut world.rig.engine, note, &mut stream_up);
    }
    api(&world.mock.url)
        .archive_object(&id, "archive-1")
        .await
        .unwrap();
    world.mock.reset_gets();
    let deadline = Instant::now() + Duration::from_secs(2);
    while world.read(&path).is_some() {
        let note = watch.recv_until(deadline).await;
        if feed::ingest(&mut world.rig.engine, note, &mut stream_up) {
            world.rig.engine.run_once().await.unwrap();
        }
    }
    assert_eq!(world.mock.get_count(), 1);
}
