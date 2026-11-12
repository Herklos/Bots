//! An in-memory Workspace, for tests.
use crate::storage::workspace::{safe_rel, DirEntry, EntryKind, Workspace};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::io;
use tokio::sync::Mutex;

#[derive(Debug)]
struct Store {
    files: HashMap<String, (Vec<u8>, u64)>,
    /// Directory keys with no file of their own. A file path is a file; a prefix is a directory.
    dirs: HashSet<String>,
}

impl Store {
    fn is_file(&self, rel: &str) -> bool {
        self.files.contains_key(rel)
    }

    fn is_dir(&self, rel: &str) -> bool {
        if self.files.contains_key(rel) {
            return false;
        }
        if self.dirs.contains(rel) {
            return true;
        }
        let prefix = format!("{rel}/");
        self.files.keys().any(|key| key.starts_with(&prefix))
            || self.dirs.iter().any(|key| key.starts_with(&prefix))
    }

    fn ancestor_is_file(&self, rel: &str) -> bool {
        let mut cursor = rel;
        while let Some((parent, _)) = cursor.rsplit_once('/') {
            if self.files.contains_key(parent) {
                return true;
            }
            cursor = parent;
        }
        false
    }

    fn remove_tree(&mut self, rel: &str) {
        let prefix = format!("{rel}/");
        self.files
            .retain(|key, _| key != rel && !key.starts_with(&prefix));
        self.dirs
            .retain(|key| key != rel && !key.starts_with(&prefix));
    }
}

#[derive(Debug)]
pub(crate) struct MemoryWorkspace {
    store: Mutex<Store>,
}

impl MemoryWorkspace {
    pub(crate) fn new() -> Self {
        Self {
            store: Mutex::new(Store {
                files: HashMap::new(),
                dirs: HashSet::new(),
            }),
        }
    }
}

fn not_dir() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "not a directory")
}

fn missing() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "missing")
}

fn direct_child<'a>(key: &'a str, prefix: &str) -> Option<(&'a str, bool)> {
    let rest = key.strip_prefix(prefix)?;
    match rest.split_once('/') {
        Some((name, _)) => Some((name, true)),
        None => Some((rest, false)),
    }
}

fn record_child(entries: &mut HashMap<String, DirEntry>, name: &str, file_modified: Option<u64>) {
    if name.is_empty() {
        return;
    }
    entries
        .entry(name.to_string())
        .and_modify(|entry| {
            if file_modified.is_none() {
                entry.is_file = false;
                entry.modified = 0;
            }
        })
        .or_insert(DirEntry {
            name: name.to_string(),
            is_file: file_modified.is_some(),
            modified: file_modified.unwrap_or(0),
        });
}

#[async_trait]
impl Workspace for MemoryWorkspace {
    async fn read(&self, rel: &str) -> io::Result<Vec<u8>> {
        safe_rel(rel)?;
        self.store
            .lock()
            .await
            .files
            .get(rel)
            .map(|(bytes, _)| bytes.clone())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "missing"))
    }

    async fn write(&self, rel: &str, bytes: &[u8], _mode: Option<u32>) -> io::Result<()> {
        safe_rel(rel)?;
        let mut store = self.store.lock().await;
        store.dirs.remove(rel);
        store
            .files
            .insert(rel.to_string(), (bytes.to_vec(), 1_700_000_000));
        Ok(())
    }

    async fn list(&self, rel: &str) -> io::Result<Vec<DirEntry>> {
        safe_rel(rel)?;
        let prefix = format!("{rel}/");
        let store = self.store.lock().await;
        let mut entries = HashMap::<String, DirEntry>::new();
        for (key, (_, modified)) in &store.files {
            if let Some((name, nested)) = direct_child(key, &prefix) {
                let modified = if nested { None } else { Some(*modified) };
                record_child(&mut entries, name, modified);
            }
        }
        for key in &store.dirs {
            if let Some((name, _)) = direct_child(key, &prefix) {
                record_child(&mut entries, name, None);
            }
        }
        let mut out: Vec<_> = entries.into_values().collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    async fn remove(&self, rel: &str) -> io::Result<()> {
        safe_rel(rel)?;
        self.store.lock().await.remove_tree(rel);
        Ok(())
    }

    fn root_for_harness(&self, slug: &str) -> String {
        format!("memory://{slug}")
    }

    async fn exists(&self, rel: &str) -> io::Result<bool> {
        safe_rel(rel)?;
        let store = self.store.lock().await;
        Ok(store.is_file(rel) || store.is_dir(rel))
    }

    async fn symlink_kind(&self, rel: &str) -> io::Result<Option<EntryKind>> {
        safe_rel(rel)?;
        let store = self.store.lock().await;
        if store.is_file(rel) {
            Ok(Some(EntryKind::File))
        } else if store.is_dir(rel) {
            Ok(Some(EntryKind::Dir))
        } else {
            Ok(None)
        }
    }

    async fn require_dir(&self, rel: &str) -> io::Result<()> {
        safe_rel(rel)?;
        let store = self.store.lock().await;
        if store.is_file(rel) || store.ancestor_is_file(rel) {
            return Err(not_dir());
        }
        if store.is_dir(rel) {
            Ok(())
        } else {
            Err(missing())
        }
    }

    async fn ensure_dir(&self, rel: &str) -> io::Result<()> {
        safe_rel(rel)?;
        let mut store = self.store.lock().await;
        if store.is_file(rel) || store.ancestor_is_file(rel) {
            return Err(not_dir());
        }
        if !store.is_dir(rel) {
            store.dirs.insert(rel.to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            ws.symlink_kind("ada/library/page.html").await.unwrap(),
            Some(EntryKind::File)
        );
        assert_eq!(
            ws.symlink_kind("ada/library").await.unwrap(),
            Some(EntryKind::Dir)
        );
        ws.require_dir("ada").await.unwrap();
        assert!(ws.exists("ada/library").await.unwrap());
        assert!(ws.read("../ada/library/page.html").await.is_err());
        ws.remove("ada/library/page.html").await.unwrap();
        assert!(ws.read("ada/library/page.html").await.is_err());
        assert_eq!(ws.root_for_harness("ada"), "memory://ada");
    }

    #[tokio::test]
    async fn dir_methods_cover_file_directory_missing_and_escape() {
        let ws = MemoryWorkspace::new();

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
        ws.ensure_dir("ada/memory").await.unwrap();
        assert!(ws.exists("ada/memory/notes").await.unwrap());
        assert_eq!(ws.symlink_kind("ada").await.unwrap(), Some(EntryKind::Dir));
        assert_ne!(
            ws.symlink_kind("ada/memory").await.unwrap(),
            Some(EntryKind::Symlink)
        );
        let listed = ws.list("ada/memory").await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "notes");
        assert!(!listed[0].is_file);

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
        assert!(ws.ensure_dir("ada/note.md/child").await.is_err());
        assert_eq!(ws.read("ada/note.md").await.unwrap(), b"hi");

        ws.ensure_dir("bea").await.unwrap();
        ws.write("bea", b"file", None).await.unwrap();
        assert_eq!(ws.symlink_kind("bea").await.unwrap(), Some(EntryKind::File));
        assert!(ws.require_dir("bea").await.is_err());

        ws.remove("ada/memory").await.unwrap();
        assert!(!ws.exists("ada/memory/notes").await.unwrap());
        assert!(ws.require_dir("ada/memory/notes").await.is_err());

        for rel in [
            "../x",
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
    }
}
