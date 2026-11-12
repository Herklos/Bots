//! `SyncedWorkspace`: [`LocalDisk`] plus a nudge after every write and remove, so the
//! engine notices a `Library` change within a second instead of on the next scan.
//! The engine itself starts later in `main`; the queue it drains is shared, so a
//! nudge with no running engine is only a queued entry, never an error.

use crate::storage::sync::Nudges;
use crate::storage::workspace::{DirEntry, EntryKind, Workspace};
use crate::storage::LocalDisk;
use async_trait::async_trait;
use std::io;
use std::sync::Arc;

#[derive(Debug)]
pub(crate) struct SyncedWorkspace {
    inner: LocalDisk,
    nudges: Arc<Nudges>,
}

impl SyncedWorkspace {
    pub(crate) fn new(inner: LocalDisk, nudges: Arc<Nudges>) -> Self {
        Self { inner, nudges }
    }

    fn nudge(&self, rel: &str) {
        self.nudges.push(rel);
    }
}

#[async_trait]
impl Workspace for SyncedWorkspace {
    async fn read(&self, rel: &str) -> io::Result<Vec<u8>> {
        self.inner.read(rel).await
    }

    async fn write(&self, rel: &str, bytes: &[u8], mode: Option<u32>) -> io::Result<()> {
        self.inner.write(rel, bytes, mode).await?;
        self.nudge(rel);
        Ok(())
    }

    async fn list(&self, rel: &str) -> io::Result<Vec<DirEntry>> {
        self.inner.list(rel).await
    }

    async fn remove(&self, rel: &str) -> io::Result<()> {
        self.inner.remove(rel).await?;
        self.nudge(rel);
        Ok(())
    }

    fn root_for_harness(&self, slug: &str) -> String {
        self.inner.root_for_harness(slug)
    }

    async fn exists(&self, rel: &str) -> io::Result<bool> {
        self.inner.exists(rel).await
    }

    async fn symlink_kind(&self, rel: &str) -> io::Result<Option<EntryKind>> {
        self.inner.symlink_kind(rel).await
    }

    async fn require_dir(&self, rel: &str) -> io::Result<()> {
        self.inner.require_dir(rel).await
    }

    async fn ensure_dir(&self, rel: &str) -> io::Result<()> {
        self.inner.ensure_dir(rel).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn nudges_after_write_and_remove() {
        let dir = tempfile::tempdir().unwrap();
        let disk = LocalDisk::new(dir.path().to_path_buf());
        let nudges = Arc::new(Nudges::default());
        let synced = SyncedWorkspace::new(disk, nudges.clone());
        synced
            .write("ada/library/a.html", b"x", None)
            .await
            .unwrap();
        synced
            .write("ada/library/b.html", b"y", None)
            .await
            .unwrap();
        synced.remove("ada/library/a.html").await.unwrap();
        // One entry per path: a remove after a write only refreshes its nudge.
        let mut queue = nudges.drain();
        assert_eq!(queue.len(), 2);
        queue.sort();
        assert_eq!(queue, vec!["ada/library/a.html", "ada/library/b.html"]);
    }
}
