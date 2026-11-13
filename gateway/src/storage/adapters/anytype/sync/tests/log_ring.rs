use super::super::log;
use super::support::World;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[tokio::test]
async fn a_decision_line_names_the_path_and_not_the_bytes() {
    let mut world = World::start("dev-log").await;
    world.live().await;
    let path = "ada/memory/note.md";
    let body = b"NOT-IN-THE-LOG\n";
    world.write(path, body);
    world.rig.until_quiet().await;
    let log = std::fs::read_to_string(world.rig.sync_dir.path().join("sync.log")).unwrap();
    assert!(log.contains("path=ada/memory/note.md"), "{log}");
    assert!(log.contains("decision=push-create"), "{log}");
    assert!(log.contains("base="), "{log}");
    assert!(log.contains("local="), "{log}");
    assert!(log.contains("remote="), "{log}");
    assert!(!log.contains("NOT-IN-THE-LOG"), "{log}");
    assert!(!log.contains("idem"), "{log}");
    #[cfg(unix)]
    assert_eq!(
        std::fs::metadata(world.rig.sync_dir.path().join("sync.log"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn the_log_ring_keeps_the_newest_lines() {
    let dir = tempfile::tempdir().unwrap();
    for index in 0..4_000 {
        log::decision(
            dir.path(),
            "ada/memory/note.md",
            "noop",
            Some("abc"),
            Some("abc"),
            Some(&format!("sha-{index:04}")),
        );
    }
    let log = std::fs::read_to_string(dir.path().join("sync.log")).unwrap();
    assert!(log.len() < 64 * 1024, "{}", log.len());
    assert!(log.contains("sha-3999"));
    assert!(!log.contains("sha-0000"));
    assert!(!log.contains("NOT-IN-THE-LOG"));
}
