use super::support::{files_with_path, Rig, World};

#[tokio::test]
async fn a_fresh_tombstone_stays_searchable() {
    let mut world = World::start("dev-gc-fresh").await;
    world.live().await;
    let path = "ada/memory/fresh.md";
    world.write(path, b"keep\n");
    world.rig.until_quiet().await;
    std::fs::remove_file(world.rig.root.path().join(path)).unwrap();
    world.rig.until_quiet().await;
    let id = files_with_path(&world.mock.url, path).await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(world.mock.set_last_modified(&id, &days_ago(1)));
    world.rig.until_quiet().await;
    let rows = files_with_path(&world.mock.url, path).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["properties"]["hb_deleted"], true);
}

#[tokio::test]
async fn an_old_tombstone_is_archived_once_and_a_race_is_tolerated() {
    let mut world = World::start("dev-gc-a").await;
    world.live().await;
    let path = "ada/memory/old.md";
    world.write(path, b"gone\n");
    world.rig.until_quiet().await;
    std::fs::remove_file(world.rig.root.path().join(path)).unwrap();
    world.rig.until_quiet().await;
    let id = files_with_path(&world.mock.url, path).await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(world.mock.set_last_modified(&id, &days_ago(40)));

    world.mock.set_archived_writes(true);
    world.rig.until_quiet().await;
    assert_eq!(
        files_with_path(&world.mock.url, path).await.len(),
        1,
        "a 409 leaves the tombstone in place"
    );
    assert!(world.rig.engine.status().last_error.is_none());

    world.mock.set_archived_writes(false);
    let mut other = Rig::open(&world.mock.url, "dev-gc-b");
    other.live().await;
    assert!(
        files_with_path(&world.mock.url, path).await.is_empty(),
        "the other device archives it; its key is not the replayed 409"
    );
    assert!(world.read(path).is_none());
}

fn days_ago(days: u64) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .saturating_sub(days * 86_400);
    let z = secs.div_euclid(86_400).cast_signed() + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097).cast_unsigned();
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let mut year = i64::try_from(yoe).unwrap() + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    if month <= 2 {
        year += 1;
    }
    let tod = secs % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}
