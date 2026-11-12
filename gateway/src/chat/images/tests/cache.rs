use super::super::{attach, png_bytes, ImageFault, Resolver, MAX_IMAGE_BYTES};
use crate::chat::transcript_tag;
use crate::storage::adapters::memory::MemoryWorkspace;
use crate::storage::workspace::{DirEntry, EntryKind, Workspace};
use async_trait::async_trait;
use serde_json::json;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct Counting {
    inner: MemoryWorkspace,
    reads: AtomicUsize,
    kinds: AtomicUsize,
}

impl Counting {
    fn new() -> Self {
        Self {
            inner: MemoryWorkspace::new(),
            reads: AtomicUsize::new(0),
            kinds: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl Workspace for Counting {
    async fn read(&self, rel: &str) -> io::Result<Vec<u8>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.inner.read(rel).await
    }
    async fn write(&self, rel: &str, bytes: &[u8], mode: Option<u32>) -> io::Result<()> {
        self.inner.write(rel, bytes, mode).await
    }
    async fn list(&self, rel: &str) -> io::Result<Vec<DirEntry>> {
        self.inner.list(rel).await
    }
    async fn remove(&self, rel: &str) -> io::Result<()> {
        self.inner.remove(rel).await
    }
    fn root_for_harness(&self, slug: &str) -> String {
        self.inner.root_for_harness(slug)
    }
    async fn exists(&self, rel: &str) -> io::Result<bool> {
        self.inner.exists(rel).await
    }
    async fn symlink_kind(&self, rel: &str) -> io::Result<Option<EntryKind>> {
        self.kinds.fetch_add(1, Ordering::SeqCst);
        self.inner.symlink_kind(rel).await
    }
    async fn require_dir(&self, rel: &str) -> io::Result<()> {
        self.inner.require_dir(rel).await
    }
    async fn ensure_dir(&self, rel: &str) -> io::Result<()> {
        self.inner.ensure_dir(rel).await
    }
}

fn view(role: &str, text: &str) -> serde_json::Value {
    json!({
        "messages": [{ "id": "m1", "role": role, "text": text, "files": [], "at": "1970-01-01T00:00:01.000Z" }],
        "events": [], "requests": [], "working": false, "pins": [], "readOnly": false, "threadId": "t"
    })
}

#[tokio::test]
async fn a_hit_does_no_file_io_and_a_late_file_appears_after_five_seconds() {
    let ws = Counting::new();
    let resolver = Resolver::new();
    let err = resolver
        .resolve(&ws, "ada", "library/chart.png", Some(0), 0)
        .await
        .unwrap_err();
    assert_eq!(err, ImageFault::Missing);
    assert_eq!(ws.reads.load(Ordering::SeqCst), 0);
    assert_eq!(ws.kinds.load(Ordering::SeqCst), 1);
    let again = resolver
        .resolve(&ws, "ada", "library/chart.png", Some(0), 4_999)
        .await
        .unwrap_err();
    assert_eq!(again, ImageFault::Missing);
    assert_eq!(ws.kinds.load(Ordering::SeqCst), 1);

    ws.write("ada/library/chart.png", &png_bytes(2, 1), None)
        .await
        .unwrap();
    let image = resolver
        .resolve(&ws, "ada", "library/chart.png", Some(0), 5_000)
        .await
        .unwrap();
    assert_eq!(image.path, "library/chart.png");
    assert_eq!(image.width, 2);
    let hit = resolver
        .resolve(&ws, "ada", "library/chart.png", Some(0), 5_001)
        .await
        .unwrap();
    assert_eq!(hit.etag, image.etag);
    assert_eq!(ws.reads.load(Ordering::SeqCst), 1);
    assert_eq!(ws.kinds.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn bytes_can_change_only_after_the_success_expires() {
    let ws = Counting::new();
    ws.write("ada/library/chart.png", &png_bytes(2, 1), None)
        .await
        .unwrap();
    let resolver = Resolver::new();
    let first = resolver
        .resolve(&ws, "ada", "library/chart.png", Some(0), 0)
        .await
        .unwrap();
    ws.write("ada/library/chart.png", &png_bytes(9, 8), None)
        .await
        .unwrap();
    let cached = resolver
        .resolve(&ws, "ada", "library/chart.png", Some(0), 10 * 60 * 1000 - 1)
        .await
        .unwrap();
    assert_eq!(cached.etag, first.etag);
    assert_eq!(cached.width, 2);
    let fresh = resolver
        .resolve(&ws, "ada", "library/chart.png", Some(0), 10 * 60 * 1000)
        .await
        .unwrap();
    assert_ne!(fresh.etag, first.etag);
    assert_eq!(fresh.width, 9);
}

#[tokio::test]
async fn an_old_message_keeps_a_failure_and_a_deleted_file_drops_out() {
    let ws = Counting::new();
    let resolver = Resolver::new();
    let err = resolver
        .resolve(&ws, "ada", "library/chart.png", Some(0), 30_001)
        .await
        .unwrap_err();
    assert_eq!(err, ImageFault::Missing);
    ws.write("ada/library/chart.png", &png_bytes(2, 1), None)
        .await
        .unwrap();
    let still = resolver
        .resolve(
            &ws,
            "ada",
            "library/chart.png",
            Some(0),
            30_001 + 10 * 60 * 1000 - 1,
        )
        .await
        .unwrap_err();
    assert_eq!(still, ImageFault::Missing);

    let found_at = 30_001 + 10 * 60 * 1000;
    let shown = resolver
        .resolve(&ws, "ada", "library/chart.png", Some(0), found_at)
        .await
        .unwrap();
    assert_eq!(shown.name, "chart.png");
    ws.remove("ada/library/chart.png").await.unwrap();
    let cached = resolver
        .resolve(
            &ws,
            "ada",
            "library/chart.png",
            Some(0),
            found_at + 10 * 60 * 1000 - 1,
        )
        .await
        .unwrap();
    assert_eq!(cached.etag, shown.etag);
    let missing = resolver
        .resolve(
            &ws,
            "ada",
            "library/chart.png",
            Some(0),
            found_at + 10 * 60 * 1000,
        )
        .await
        .unwrap_err();
    assert_eq!(missing, ImageFault::Missing);

    let mut body = view("bot", "![chart](library/chart.png)");
    attach(
        &mut body,
        &resolver,
        &ws,
        "ada",
        "",
        found_at + 20 * 60 * 1000,
    )
    .await;
    assert_eq!(body["messages"][0]["text"], "[chart]");
    assert!(body["messages"][0]["files"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn a_bot_reply_gains_a_file_and_the_next_poll_is_the_same() {
    let ws = Counting::new();
    ws.write("ada/library/chart.png", &png_bytes(4, 3), None)
        .await
        .unwrap();
    let resolver = Resolver::new();
    let mut body = view(
        "bot",
        "See ![chart](library/chart.png) and ![remote](https://example.test/a.png)",
    );
    let mut user = view("user", "![mine](https://example.test/a.png)");
    attach(&mut body, &resolver, &ws, "ada", "", 1_000).await;
    attach(&mut user, &resolver, &ws, "ada", "", 1_000).await;
    assert_eq!(body["messages"][0]["text"], "See  and [remote]");
    assert_eq!(body["messages"][0]["files"][0]["kind"], "image");
    assert_eq!(body["messages"][0]["files"][0]["path"], "library/chart.png");
    assert_eq!(body["messages"][0]["files"][0]["name"], "chart.png");
    assert_eq!(
        user["messages"][0]["text"],
        "![mine](https://example.test/a.png)"
    );
    let tag = transcript_tag(&body);
    let reads = ws.reads.load(Ordering::SeqCst);
    attach(&mut body, &resolver, &ws, "ada", "", 1_001).await;
    assert_eq!(transcript_tag(&body), tag);
    assert_eq!(ws.reads.load(Ordering::SeqCst), reads);
    assert_eq!(ws.kinds.load(Ordering::SeqCst), reads);
}

#[tokio::test]
async fn eight_images_is_the_limit_and_the_wrong_bytes_are_refused() {
    let ws = Counting::new();
    for n in 1..=9 {
        ws.write(&format!("ada/library/{n}.png"), &png_bytes(1, 1), None)
            .await
            .unwrap();
    }
    let mut huge = png_bytes(1, 1);
    huge.resize(MAX_IMAGE_BYTES + 1, 0);
    ws.write("ada/library/huge.png", &huge, None).await.unwrap();
    ws.write("ada/library/note.png", b"hello", None)
        .await
        .unwrap();
    let resolver = Resolver::new();
    assert_eq!(
        resolver
            .resolve(&ws, "ada", "library/huge.png", Some(0), 0)
            .await
            .unwrap_err(),
        ImageFault::TooBig
    );
    assert_eq!(
        resolver
            .resolve(&ws, "ada", "library/note.png", Some(0), 0)
            .await
            .unwrap_err(),
        ImageFault::NotImage
    );
    let mut text = String::new();
    for n in 1..=9 {
        text.push_str(&format!("![n](library/{n}.png)"));
    }
    let mut body = view("bot", &text);
    attach(&mut body, &resolver, &ws, "ada", "", 1_000).await;
    assert_eq!(body["messages"][0]["files"].as_array().unwrap().len(), 8);
    assert!(body["messages"][0]["text"]
        .as_str()
        .unwrap()
        .contains("[n]"));
}

#[tokio::test]
async fn the_cache_stops_at_256_entries() {
    let ws = Counting::new();
    let resolver = Resolver::new();
    for n in 0..257 {
        let path = format!("nope/{n}");
        let _ = resolver
            .resolve(&ws, "ada", &path, None, u64::try_from(n).unwrap())
            .await;
    }
    assert_eq!(resolver.len(), 256);
    assert_eq!(ws.reads.load(Ordering::SeqCst), 0);
}

#[cfg(unix)]
#[tokio::test]
async fn a_symlink_is_not_the_file_it_points_at() {
    let dir = tempfile::tempdir().unwrap();
    let ws = crate::storage::LocalDisk::new(dir.path().to_path_buf());
    std::fs::create_dir_all(dir.path().join("ada/library")).unwrap();
    std::fs::write(dir.path().join("ada/secret.bin"), b"secret-bytes").unwrap();
    std::os::unix::fs::symlink("../secret.bin", dir.path().join("ada/library/chart.png")).unwrap();
    let resolver = Resolver::new();
    let err = resolver
        .resolve(&ws, "ada", "library/chart.png", Some(0), 0)
        .await
        .unwrap_err();
    assert_eq!(err, ImageFault::Link);
}
