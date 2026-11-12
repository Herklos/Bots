//! Where a Bot's files live. The harness and the folder location vary independently
//! (`docs/backends.md`). Library records, files, and MCP connectors go through this
//! trait. The implementations live under `storage/adapters`.
use async_trait::async_trait;
use std::io;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub is_file: bool,
    /// Unix seconds, or 0 when the store has no timestamp. Callers must not invent an age.
    pub modified: u64,
}

/// Relative paths only, under one root. `..`, absolute paths, and empty segments are refused.
pub(crate) fn safe_rel(rel: &str) -> io::Result<()> {
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
pub trait Workspace: std::fmt::Debug + Send + Sync {
    async fn read(&self, rel: &str) -> io::Result<Vec<u8>>;
    async fn write(&self, rel: &str, bytes: &[u8], mode: Option<u32>) -> io::Result<()>;
    async fn list(&self, rel: &str) -> io::Result<Vec<DirEntry>>;
    async fn remove(&self, rel: &str) -> io::Result<()>;
    fn root_for_harness(&self, slug: &str) -> String;
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
}
