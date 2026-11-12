use super::super::scan::{scan, Shortcut, SETTLE};
use super::super::sha256_hex;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn write_at(path: &Path, bytes: &[u8], mtime: SystemTime) -> i64 {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(mtime).unwrap();
    drop(file);
    let meta = std::fs::symlink_metadata(path).unwrap();
    let nanos = meta
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    i64::try_from(nanos).unwrap()
}

#[test]
fn changed_file_is_rehashed_and_unchanged_file_is_not() {
    let root = tempfile::tempdir().unwrap();
    let now = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let old = now - Duration::from_secs(3_600);
    let newer = now - Duration::from_secs(60);
    let same = root.path().join("ada/memory/same.md");
    let changed = root.path().join("ada/memory/changed.md");
    let same_mtime = write_at(&same, b"same", old);
    write_at(&changed, b"changed-body", newer);

    let mut cache = BTreeMap::new();
    cache.insert(
        "ada/memory/same.md".into(),
        Shortcut {
            size: 4,
            mtime_ns: same_mtime,
            sha: "cached-not-a-real-hash".into(),
        },
    );
    cache.insert(
        "ada/memory/changed.md".into(),
        Shortcut {
            size: 1,
            mtime_ns: same_mtime - 1,
            sha: "old".into(),
        },
    );

    let scanned = scan(root.path(), &cache, now, SETTLE).unwrap();
    assert_eq!(
        scanned.files["ada/memory/same.md"].sha,
        "cached-not-a-real-hash"
    );
    assert_eq!(
        scanned.files["ada/memory/changed.md"].sha,
        sha256_hex(b"changed-body")
    );
    assert!(scanned.reused.contains("ada/memory/same.md"));
    assert!(!scanned.hashed.contains("ada/memory/same.md"));
    assert!(scanned.hashed.contains("ada/memory/changed.md"));
    assert!(!scanned.reused.contains("ada/memory/changed.md"));
}

#[test]
fn a_file_touched_within_one_second_waits() {
    let root = tempfile::tempdir().unwrap();
    let anchor = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let fresh_ns = write_at(&root.path().join("ada/memory/fresh.md"), b"fresh", anchor);
    // `now` is derived from the mtime the filesystem stored, so a coarse clock
    // cannot push the fresh file outside the settle window.
    let now = UNIX_EPOCH
        + Duration::from_nanos(u64::try_from(fresh_ns).unwrap())
        + Duration::from_millis(200);
    write_at(
        &root.path().join("ada/memory/old.md"),
        b"old",
        now - SETTLE - Duration::from_secs(5),
    );
    let scanned = scan(root.path(), &BTreeMap::new(), now, SETTLE).unwrap();
    assert!(scanned.unsettled.contains("ada/memory/fresh.md"));
    assert!(!scanned.files.contains_key("ada/memory/fresh.md"));
    assert!(!scanned.hashed.contains("ada/memory/fresh.md"));
    assert!(scanned.files.contains_key("ada/memory/old.md"));
    assert_eq!(scanned.files["ada/memory/old.md"].sha, sha256_hex(b"old"));
}

#[test]
fn scan_keeps_only_allowed_files() {
    let root = tempfile::tempdir().unwrap();
    let now = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let old = now - Duration::from_secs(60);
    for rel in [
        "ada/memory/note.md",
        ".botapp/shared-memory.md",
        "ada/AGENTS.md",
        "ada/.env",
        "ada/.bot/bot.json",
        "ada/README.md",
    ] {
        write_at(&root.path().join(rel), b"x", old);
    }
    let scanned = scan(root.path(), &BTreeMap::new(), now, SETTLE).unwrap();
    let keys: Vec<_> = scanned.files.keys().cloned().collect();
    assert_eq!(
        keys,
        vec![
            ".botapp/shared-memory.md".to_string(),
            "ada/memory/note.md".to_string(),
        ]
    );
    assert!(scanned.unsettled.is_empty());
}

#[cfg(unix)]
#[test]
fn scan_does_not_follow_symlinks() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let now = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let old = now - Duration::from_secs(60);
    std::fs::create_dir_all(outside.path().join("nested")).unwrap();
    std::fs::write(outside.path().join("nested/note.md"), "secret").unwrap();
    std::fs::create_dir_all(root.path().join("ada")).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("ada/memory")).unwrap();
    write_at(&root.path().join("ada/skills/real.md"), b"real", old);
    std::os::unix::fs::symlink("real.md", root.path().join("ada/skills/link.md")).unwrap();

    let scanned = scan(root.path(), &BTreeMap::new(), now, SETTLE).unwrap();
    assert_eq!(
        scanned.files.keys().cloned().collect::<Vec<_>>(),
        vec!["ada/skills/real.md".to_string()]
    );
    assert!(!scanned.hashed.contains("ada/memory/nested/note.md"));
    assert!(!scanned.files.contains_key("ada/skills/link.md"));
}
