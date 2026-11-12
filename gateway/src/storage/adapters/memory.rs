//! An in-memory Workspace, for tests.
use crate::storage::workspace::{safe_rel, DirEntry, Workspace};
use async_trait::async_trait;
use std::collections::HashMap;
use std::io;
use tokio::sync::Mutex;

#[derive(Debug)]
pub(crate) struct MemoryWorkspace {
    files: Mutex<HashMap<String, (Vec<u8>, u64)>>,
}

impl MemoryWorkspace {
    pub(crate) fn new() -> Self {
        Self {
            files: Mutex::new(HashMap::new()),
        }
    }
}

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
}
