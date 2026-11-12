//! The Workspace adapter the gateway runs: a folder on this machine.
use super::atomic::write_atomic;
use crate::storage::workspace::{safe_rel, DirEntry, EntryKind, Workspace};
use async_trait::async_trait;
use std::io;
use std::path::{Path, PathBuf};

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

    /// `canonicalize` follows intermediate symlinks, so a directory reached
    /// through a link that leaves this root is refused here.
    async fn contained(&self, path: &Path) -> io::Result<()> {
        let root = tokio::fs::canonicalize(&self.root).await?;
        let dir = tokio::fs::canonicalize(path).await?;
        if dir.starts_with(&root) {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path escapes the workspace",
            ))
        }
    }

    /// A missing path is created only under real directories. A symlink ancestor
    /// would make `create_dir_all` write outside this root, or into another Bot.
    async fn real_parent_chain(&self, rel: &str) -> io::Result<()> {
        let parts: Vec<&str> = rel.split('/').collect();
        if parts.len() <= 1 {
            return Ok(());
        }
        let mut cursor = self.root.clone();
        for part in &parts[..parts.len() - 1] {
            cursor.push(part);
            match tokio::fs::symlink_metadata(&cursor).await {
                Ok(meta) if meta.file_type().is_dir() => {}
                Ok(_) => return Err(not_dir()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(error),
            }
        }
        self.contained(&cursor).await
    }
}

fn entry_kind(meta: &std::fs::Metadata) -> EntryKind {
    let kind = meta.file_type();
    if kind.is_symlink() {
        EntryKind::Symlink
    } else if kind.is_dir() {
        EntryKind::Dir
    } else {
        EntryKind::File
    }
}

fn not_dir() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "not a directory")
}

fn missing() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "missing")
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

    async fn exists(&self, rel: &str) -> io::Result<bool> {
        let path = self.full(rel)?;
        match tokio::fs::symlink_metadata(&path).await {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    async fn symlink_kind(&self, rel: &str) -> io::Result<Option<EntryKind>> {
        let path = self.full(rel)?;
        match tokio::fs::symlink_metadata(&path).await {
            Ok(meta) => Ok(Some(entry_kind(&meta))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn require_dir(&self, rel: &str) -> io::Result<()> {
        let path = self.full(rel)?;
        match tokio::fs::symlink_metadata(&path).await {
            Ok(meta) if meta.file_type().is_dir() => self.contained(&path).await,
            Ok(_) => Err(not_dir()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Err(missing()),
            Err(error) => Err(error),
        }
    }

    async fn ensure_dir(&self, rel: &str) -> io::Result<()> {
        let path = self.full(rel)?;
        match tokio::fs::symlink_metadata(&path).await {
            Ok(meta) if meta.file_type().is_dir() => return self.contained(&path).await,
            Ok(_) => return Err(not_dir()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        self.real_parent_chain(rel).await?;
        tokio::fs::create_dir_all(&path).await?;
        self.contained(&path).await
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

    #[tokio::test]
    async fn dir_methods_cover_file_directory_missing_and_escape() {
        let dir = tempfile::tempdir().unwrap();
        let ws = LocalDisk::new(dir.path().to_path_buf());

        assert!(!ws.exists("ada").await.unwrap());
        assert_eq!(ws.symlink_kind("ada").await.unwrap(), None);
        assert_eq!(
            ws.require_dir("ada").await.unwrap_err().kind(),
            io::ErrorKind::NotFound
        );

        ws.ensure_dir("ada/memory/notes").await.unwrap();
        ws.require_dir("ada").await.unwrap();
        ws.require_dir("ada/memory").await.unwrap();
        ws.require_dir("ada/memory/notes").await.unwrap();
        ws.ensure_dir("ada/memory/notes").await.unwrap();
        assert!(ws.exists("ada/memory/notes").await.unwrap());
        assert_eq!(
            ws.symlink_kind("ada/memory/notes").await.unwrap(),
            Some(EntryKind::Dir)
        );
        assert!(dir.path().join("ada/memory/notes").is_dir());

        ws.write("ada/note.md", b"hi", None).await.unwrap();
        assert!(ws.exists("ada/note.md").await.unwrap());
        assert_eq!(
            ws.symlink_kind("ada/note.md").await.unwrap(),
            Some(EntryKind::File)
        );
        assert_eq!(
            ws.require_dir("ada/note.md").await.unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            ws.ensure_dir("ada/note.md").await.unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            std::fs::read(dir.path().join("ada/note.md")).unwrap(),
            b"hi"
        );

        let probe = format!("hbots-escape-{}", std::process::id());
        let escaped = format!("../{probe}");
        for rel in [
            escaped.as_str(),
            "ada/../../etc/passwd",
            "/etc/passwd",
            "ada/./note",
            "",
        ] {
            assert!(ws.exists(rel).await.is_err(), "{rel}");
            assert!(ws.symlink_kind(rel).await.is_err(), "{rel}");
            assert!(ws.require_dir(rel).await.is_err(), "{rel}");
            assert!(ws.ensure_dir(rel).await.is_err(), "{rel}");
        }
        assert!(!dir.path().parent().unwrap().join(probe).exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlinks_are_refused_and_reads_still_follow() {
        let dir = tempfile::tempdir().unwrap();
        let ws = LocalDisk::new(dir.path().to_path_buf());
        ws.write("bea/memory/MEMORY.md", b"secret", None)
            .await
            .unwrap();
        std::os::unix::fs::symlink("note.md", dir.path().join("bea/link.md")).unwrap();
        std::fs::write(dir.path().join("bea/note.md"), b"leaf").unwrap();
        std::os::unix::fs::symlink("missing", dir.path().join("bea/dangling")).unwrap();

        assert!(ws.exists("bea/link.md").await.unwrap());
        assert_eq!(
            ws.symlink_kind("bea/link.md").await.unwrap(),
            Some(EntryKind::Symlink)
        );
        assert_eq!(
            ws.require_dir("bea/link.md").await.unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            ws.ensure_dir("bea/link.md").await.unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            std::fs::read(dir.path().join("bea/note.md")).unwrap(),
            b"leaf"
        );
        assert_eq!(
            ws.symlink_kind("bea/dangling").await.unwrap(),
            Some(EntryKind::Symlink)
        );
        assert!(ws.exists("bea/dangling").await.unwrap());
        assert!(ws.require_dir("bea/dangling").await.is_err());
        assert!(ws.ensure_dir("bea/dangling").await.is_err());

        std::os::unix::fs::symlink("bea", dir.path().join("ada")).unwrap();
        assert_eq!(
            ws.symlink_kind("ada").await.unwrap(),
            Some(EntryKind::Symlink)
        );
        assert!(ws.exists("ada").await.unwrap());
        assert_eq!(
            ws.require_dir("ada").await.unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            ws.ensure_dir("ada").await.unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(ws.ensure_dir("ada/extra").await.is_err());
        assert!(!dir.path().join("bea/extra").exists());
        assert_eq!(ws.read("ada/memory/MEMORY.md").await.unwrap(), b"secret");

        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(outside.path().join("notes")).unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("escape")).unwrap();
        assert!(ws.require_dir("escape").await.is_err());
        assert_eq!(
            ws.require_dir("escape/notes").await.unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(ws.ensure_dir("escape").await.is_err());
        assert!(ws.ensure_dir("escape/fresh").await.is_err());
        assert!(!outside.path().join("fresh").exists());
    }
}
