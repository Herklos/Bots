use super::super::outbox::{Item, Op, REFUSE_AT, WARN_AT};
use super::super::status::SyncState;
use super::support::World;

#[tokio::test]
async fn the_outbox_warns_at_one_thousand_and_refuses_past_ten_thousand() {
    let mut world = World::start("dev-cap").await;
    world.live().await;
    for n in 0..WARN_AT - 1 {
        world.rig.engine.outbox.push(item(n)).unwrap();
    }
    assert!(!world.rig.engine.status().outbox_warn);
    world.rig.engine.outbox.push(item(WARN_AT - 1)).unwrap();
    assert!(world.rig.engine.status().outbox_warn);
    for n in WARN_AT..REFUSE_AT {
        world.rig.engine.outbox.push(item(n)).unwrap();
    }
    assert_eq!(world.rig.engine.outbox.items().len(), REFUSE_AT);
    world.write("ada/memory/extra.md", b"more\n");
    world.mock.shutdown().await;
    let cycle = world.rig.engine.run_once().await.unwrap();
    assert_eq!(cycle.status.state, SyncState::Offline, "{cycle:?}");
    assert_eq!(world.rig.engine.outbox.items().len(), REFUSE_AT);
    assert!(world
        .rig
        .engine
        .outbox
        .items()
        .iter()
        .all(|queued| queued.path != "ada/memory/extra.md"));
    assert!(world.rig.engine.status().outbox_warn);
}

fn item(n: usize) -> Item {
    Item::new(
        format!("id-{n}"),
        &format!("ada/memory/q{n}.md"),
        Op::Create,
        &format!("sha-{n}"),
        "dev-cap",
    )
}
