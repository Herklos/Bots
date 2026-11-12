//! The Workspace adapter the gateway runs: a folder on this machine.
use super::atomic::write_atomic;
use crate::storage::workspace::{safe_rel, DirEntry, Workspace};
use async_trait::async_trait;
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) struct LocalDisk {
    root: PathBuf,
}

impl LocalDisk {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn full(&self, rel: &str) -> io::Result<PathBuf> {
        safe_rel(rel)?;
        Ok(self.root.join(rel))
    }
}

#[async_trait]
impl Workspace for LocalDisk {
    async fn read(&self, rel: &str) -> io::Result<Vec<u8>> {
        tokio::fs::read(self.full(rel)?).await
    }

    async fn write(&self, rel: &str, bytes: &[u8], mode: Option<u32>) -> io::Result<()> {
        write_atomic(&self.full(rel)?, bytes, mode).await
    }

    async fn list(&self, rel: &str) -> io::Result<Vec<DirEntry>> {
        let dir = self.full(rel)?;
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut out = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.is_empty() || name == "." || name == ".." || name.contains('/') {
                continue;
            }
            let meta = entry.metadata().await?;
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            out.push(DirEntry {
                name,
                is_file: meta.is_file(),
                modified,
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    async fn remove(&self, rel: &str) -> io::Result<()> {
        let path = self.full(rel)?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(_) => tokio::fs::remove_dir_all(&path).await,
        }
    }

    fn root_for_harness(&self, slug: &str) -> String {
        self.root.join(slug).display().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_disk_writes_under_its_root() {
        let dir = tempfile::tempdir().unwrap();
        let ws = LocalDisk::new(dir.path().to_path_buf());
        ws.write("ada/.bot/library.json", b"{}\n", Some(0o600))
            .await
            .unwrap();
        let text = std::fs::read_to_string(dir.path().join("ada/.bot/library.json")).unwrap();
        assert_eq!(text, "{}\n");
        let listed = ws.list("ada/.bot").await.unwrap();
        assert_eq!(listed[0].name, "library.json");
        assert!(ws.write("../outside", b"no", None).await.is_err());
        assert!(!dir.path().join("outside").exists());
    }
}
