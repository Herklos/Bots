use super::super::history::{record, revisions, KEEP_FOR, KEEP_REVISIONS};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn set_mtime(path: &Path, mtime: SystemTime) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(mtime).unwrap();
}

async fn write_many(root: &Path, count: usize) {
    for index in 0..count {
        record(root, "ada/memory/a.md", &[u8::try_from(index).unwrap_or(0)])
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn record_stores_the_bytes_for_that_path() {
    let root = tempfile::tempdir().unwrap();
    let path = record(root.path(), "ada/memory/a.md", b"before")
        .await
        .unwrap();
    record(root.path(), "ada/memory/b.md", b"other")
        .await
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"before");
    #[cfg(unix)]
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(revisions(root.path(), "ada/memory/a.md").unwrap().len(), 1);
    assert_eq!(revisions(root.path(), "ada/memory/b.md").unwrap().len(), 1);
    let other = revisions(root.path(), "ada/memory/b.md").unwrap();
    assert_eq!(std::fs::read(&other[0].path).unwrap(), b"other");
}

#[tokio::test]
async fn ring_keeps_twenty_when_older_than_fourteen_days() {
    let root = tempfile::tempdir().unwrap();
    write_many(root.path(), KEEP_REVISIONS + 5).await;
    let now = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let old = now
        .checked_sub(KEEP_FOR + Duration::from_secs(24 * 60 * 60))
        .unwrap();
    for revision in revisions(root.path(), "ada/memory/a.md").unwrap() {
        set_mtime(&revision.path, old);
    }
    super::super::history::prune(root.path(), "ada/memory/a.md", now).unwrap();
    assert_eq!(
        revisions(root.path(), "ada/memory/a.md").unwrap().len(),
        KEEP_REVISIONS
    );
}

#[tokio::test]
async fn ring_keeps_every_revision_inside_fourteen_days() {
    let root = tempfile::tempdir().unwrap();
    let count = KEEP_REVISIONS + 5;
    write_many(root.path(), count).await;
    let now = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let recent = now.checked_sub(Duration::from_secs(24 * 60 * 60)).unwrap();
    for revision in revisions(root.path(), "ada/memory/a.md").unwrap() {
        set_mtime(&revision.path, recent);
    }
    super::super::history::prune(root.path(), "ada/memory/a.md", now).unwrap();
    assert_eq!(
        revisions(root.path(), "ada/memory/a.md").unwrap().len(),
        count
    );
}

#[tokio::test]
async fn ring_keeps_the_larger_of_the_count_and_the_window() {
    let root = tempfile::tempdir().unwrap();
    let recent_count = KEEP_REVISIONS + 2;
    let old_count = 8;
    write_many(root.path(), recent_count + old_count).await;
    let now = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let recent = now.checked_sub(Duration::from_secs(24 * 60 * 60)).unwrap();
    let old = now
        .checked_sub(KEEP_FOR + Duration::from_secs(24 * 60 * 60))
        .unwrap();
    let mut files = revisions(root.path(), "ada/memory/a.md").unwrap();
    files.sort_by(|left, right| left.path.cmp(&right.path));
    for revision in files.iter().take(old_count) {
        set_mtime(&revision.path, old);
    }
    for revision in files.iter().skip(old_count) {
        set_mtime(&revision.path, recent);
    }
    super::super::history::prune(root.path(), "ada/memory/a.md", now).unwrap();
    assert_eq!(
        revisions(root.path(), "ada/memory/a.md").unwrap().len(),
        recent_count
    );
}
