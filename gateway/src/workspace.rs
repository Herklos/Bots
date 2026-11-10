//! Where a Bot's files live. The harness and the folder location vary independently
//! (`docs/backends.md`). Library records, files, and MCP connectors go through this
//! adapter. `LocalDisk` is the implementation the gateway runs.
use async_trait::async_trait;
#[cfg(test)]
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
#[cfg(test)]
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub is_file: bool,
    /// Unix seconds, or 0 when the store has no timestamp. Callers must not invent an age.
    pub modified: u64,
}

/// Relative paths only, under one root. `..`, absolute paths, and empty segments are refused.
pub fn safe_rel(rel: &str) -> io::Result<()> {
    if rel.is_empty()
        || rel.starts_with('/')
        || rel.contains('\\')
        || rel
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path escapes the workspace",
        ));
    }
    Ok(())
}

#[async_trait]
pub trait Workspace: Send + Sync {
    async fn read(&self, rel: &str) -> io::Result<Vec<u8>>;
    async fn write(&self, rel: &str, bytes: &[u8], mode: Option<u32>) -> io::Result<()>;
    async fn list(&self, rel: &str) -> io::Result<Vec<DirEntry>>;
    async fn remove(&self, rel: &str) -> io::Result<()>;
    fn root_for_harness(&self, slug: &str) -> String;
}

pub struct LocalDisk {
    root: PathBuf,
}

impl LocalDisk {
    pub fn new(root: PathBuf) -> Self {
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
        crate::store::write_atomic(&self.full(rel)?, bytes, mode).await
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

/// In-memory store for tests. Keys are the relative paths `safe_rel` accepts.
#[cfg(test)]
pub struct MemoryWorkspace {
    files: Mutex<HashMap<String, (Vec<u8>, u64)>>,
}

#[cfg(test)]
impl MemoryWorkspace {
    pub fn new() -> Self {
        Self {
            files: Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(test)]
#[async_trait]
impl Workspace for MemoryWorkspace {
    async fn read(&self, rel: &str) -> io::Result<Vec<u8>> {
        safe_rel(rel)?;
        self.files
            .lock()
            .await
            .get(rel)
            .map(|(b, _)| b.clone())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "missing"))
    }

    async fn write(&self, rel: &str, bytes: &[u8], _mode: Option<u32>) -> io::Result<()> {
        safe_rel(rel)?;
        self.files
            .lock()
            .await
            .insert(rel.to_string(), (bytes.to_vec(), 1_700_000_000));
        Ok(())
    }

    async fn list(&self, rel: &str) -> io::Result<Vec<DirEntry>> {
        safe_rel(rel)?;
        let prefix = format!("{rel}/");
        let files = self.files.lock().await;
        let mut dirs = HashMap::<String, DirEntry>::new();
        for (key, (_, modified)) in files.iter() {
            let Some(rest) = key.strip_prefix(&prefix) else {
                continue;
            };
            let (name, nested) = match rest.split_once('/') {
                Some((name, _)) => (name, true),
                None => (rest, false),
            };
            if name.is_empty() {
                continue;
            }
            dirs.entry(name.to_string())
                .and_modify(|e| {
                    if nested {
                        e.is_file = false;
                    }
                })
                .or_insert(DirEntry {
                    name: name.to_string(),
                    is_file: !nested,
                    modified: if nested { 0 } else { *modified },
                });
        }
        let mut out: Vec<_> = dirs.into_values().collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    async fn remove(&self, rel: &str) -> io::Result<()> {
        safe_rel(rel)?;
        let mut files = self.files.lock().await;
        let prefix = format!("{rel}/");
        files.retain(|k, _| k != rel && !k.starts_with(&prefix));
        Ok(())
    }

    fn root_for_harness(&self, slug: &str) -> String {
        format!("memory://{slug}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_paths_that_escape() {
        assert!(safe_rel("ada/library/note.html").is_ok());
        assert!(safe_rel("../x").is_err());
        assert!(safe_rel("ada/../../etc/passwd").is_err());
        assert!(safe_rel("/etc/passwd").is_err());
        assert!(safe_rel("ada//note").is_err());
        assert!(safe_rel("").is_err());
        assert!(safe_rel("ada/./note").is_err());
    }

    #[tokio::test]
    async fn memory_round_trip_stays_in_the_folder() {
        let ws = MemoryWorkspace::new();
        ws.write("ada/library/page.html", b"<p>hi</p>", Some(0o644))
            .await
            .unwrap();
        let listed = ws.list("ada/library").await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "page.html");
        assert!(listed[0].is_file);
        assert_eq!(
            ws.read("ada/library/page.html").await.unwrap(),
            b"<p>hi</p>"
        );
        assert!(ws.read("../ada/library/page.html").await.is_err());
        ws.remove("ada/library/page.html").await.unwrap();
        assert!(ws.read("ada/library/page.html").await.is_err());
        assert_eq!(ws.root_for_harness("ada"), "memory://ada");
    }

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
