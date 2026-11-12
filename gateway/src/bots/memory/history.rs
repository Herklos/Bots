//! Preimages kept before a memory file changes. A failure here does not block the write.
use super::MemoryStore;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

const KEEP: usize = 50;
const KEEP_SECS: u64 = 30 * 24 * 3600;

pub(super) async fn write_preimage(
    store: &MemoryStore,
    dir: &str,
    label: &str,
    before: &str,
    now: &str,
) -> io::Result<()> {
    let stamp = now.replace([':', '.'], "-");
    let id = uuid::Uuid::new_v4().simple().to_string();
    let name = format!("{stamp}-{label}-{}.md", &id[..8]);
    store
        .write_text(&format!("{dir}/{name}"), before, Some(0o600))
        .await?;
    prune(store, dir).await
}

async fn prune(store: &MemoryStore, dir: &str) -> io::Result<()> {
    let mut files: Vec<_> = store
        .ws
        .list(dir)
        .await?
        .into_iter()
        .filter(|entry| entry.is_file && is_md(&entry.name))
        .collect();
    files.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| b.name.cmp(&a.name))
    });
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let cutoff = now.saturating_sub(KEEP_SECS);
    for (index, entry) in files.iter().enumerate() {
        // Keep the newest 50, and anything newer than 30 days. The larger set wins.
        if index < KEEP || entry.modified >= cutoff {
            continue;
        }
        store.ws.remove(&format!("{dir}/{}", entry.name)).await?;
    }
    Ok(())
}

fn is_md(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("md"))
}
