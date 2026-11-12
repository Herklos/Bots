use super::super::watch::{follow, Pace, BACKOFF_MIN};
use super::super::ReviewState;
use super::support::{snap, task, Step, World};
use std::sync::atomic::Ordering;
use std::time::Duration;

#[tokio::test]
async fn reconnect_mid_flight_holds_each_new_routine_once() {
    let first = snap(vec![task("a", 3_600_000)]);
    let second = snap(vec![task("a", 3_600_000), task("b", 3_600_000)]);
    let (world, _dir) = World::open(
        vec![
            Step::Snaps(vec![first]),
            Step::Snaps(vec![second]),
            Step::Stop,
        ],
        0,
    )
    .await;
    follow(&world, Pace::production()).await;
    let ids: Vec<_> = world
        .fake
        .writes()
        .iter()
        .map(|body| body["id"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(world.fake.watches.load(Ordering::SeqCst), 3);
    assert_eq!(
        *world.pauses.lock().expect("pauses"),
        vec![BACKOFF_MIN, BACKOFF_MIN]
    );
    let store = world.file.load().await.unwrap();
    assert_eq!(store.items["a"].state, ReviewState::Pending);
    assert_eq!(store.items["b"].state, ReviewState::Pending);
    assert_eq!(world.notices.load().await.unwrap().len(), 2);
}

#[tokio::test]
async fn offline_at_the_start_does_not_reconcile_until_a_snapshot_arrives() {
    let doc = snap(vec![task("t", 3_600_000)]);
    let (world, _dir) = World::open(vec![Step::Fail, Step::Snaps(vec![doc]), Step::Stop], 2).await;
    follow(&world, Pace::production()).await;
    assert_eq!(world.fake.watches.load(Ordering::SeqCst), 3);
    assert_eq!(world.fake.writes().len(), 1);
    assert_eq!(
        *world.pauses.lock().expect("pauses"),
        vec![
            Duration::from_secs(1),
            Duration::from_secs(2),
            Duration::from_secs(4),
            Duration::from_secs(1),
        ]
    );
    assert_eq!(
        world.file.load().await.unwrap().items["t"].state,
        ReviewState::Pending
    );
}

#[tokio::test]
async fn a_harness_without_routines_is_not_watched() {
    let (mut world, _dir) =
        World::open(vec![Step::Snaps(vec![snap(vec![task("t", 3_600_000)])])], 0).await;
    world.supported = false;
    follow(&world, Pace::production()).await;
    assert_eq!(world.fake.watches.load(Ordering::SeqCst), 0);
    assert!(world.fake.writes().is_empty());

    let (world, _dir) = World::open(vec![Step::Unsupported], 0).await;
    follow(&world, Pace::production()).await;
    assert_eq!(world.fake.watches.load(Ordering::SeqCst), 1);
    assert!(world.fake.writes().is_empty());
}
