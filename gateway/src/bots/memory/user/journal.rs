//! Finish a user-memory journal left behind between the file write and the state write.
use super::{
    checked_path, file_rel, journal_dir, lock_for, read_text, remove_file, slug_ok, state_rel,
    write_state, UserState,
};
use crate::bots::memory::state::revision_of;
use serde::Deserialize;
use std::io;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JournalFile {
    file: String,
    before_hash: String,
    after: String,
    state_file: String,
    after_state: UserState,
}

/// Finish journals left behind for one Bot's user facts.
pub(crate) async fn recover(root: &Path, slug: &str) -> io::Result<()> {
    slug_ok(slug)?;
    let _guard = lock_for(slug).lock_owned().await;
    let dir = journal_dir(root, slug);
    let mut names = match tokio::fs::read_dir(&dir).await {
        Ok(mut entries) => {
            let mut names = Vec::new();
            while let Some(entry) = entries.next_entry().await? {
                let name = entry.file_name().to_string_lossy().into_owned();
                if is_json(&name) && !name.starts_with("failed-") {
                    names.push(name);
                }
            }
            names
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };
    names.sort();
    for name in names {
        let path = dir.join(&name);
        let raw = match tokio::fs::read_to_string(&path).await {
            Ok(raw) => raw,
            Err(err) => {
                tracing::warn!("user memory journal {name} could not be read: {err}");
                continue;
            }
        };
        if let Err(err) = apply_journal(root, slug, &dir, &name, &raw).await {
            tracing::warn!("user memory journal {name} was not applied: {err}");
        }
    }
    Ok(())
}

async fn apply_journal(
    root: &Path,
    slug: &str,
    dir: &Path,
    name: &str,
    raw: &str,
) -> io::Result<()> {
    let journal = match serde_json::from_str::<JournalFile>(raw) {
        Ok(journal) if targets_ok(&journal, slug) => journal,
        _ => {
            move_failed(dir, name, raw).await;
            tracing::info!("user memory journal {name} is unreadable and was kept for review");
            return Ok(());
        }
    };
    let file_path = checked_path(root, &file_rel(slug)).await?;
    let state_path = checked_path(root, &state_rel(slug)).await?;
    let current = read_text(&file_path).await?.unwrap_or_default();
    let current_hash = revision_of(&current);
    let after_hash = revision_of(&journal.after);
    if current_hash == journal.before_hash {
        crate::storage::write_atomic(&file_path, journal.after.as_bytes(), Some(0o644)).await?;
        write_state(&state_path, &journal.after_state).await?;
        remove_file(&dir.join(name)).await?;
        tracing::info!(file = %journal.file, "user memory journal recovered");
    } else if current_hash == after_hash {
        write_state(&state_path, &journal.after_state).await?;
        remove_file(&dir.join(name)).await?;
        tracing::info!(file = %journal.file, "user memory journal completed state");
    } else {
        move_failed(dir, name, raw).await;
        tracing::info!(
            file = %journal.file,
            "user memory journal refused: the file changed outside the store"
        );
    }
    Ok(())
}

fn targets_ok(journal: &JournalFile, slug: &str) -> bool {
    journal.file == file_rel(slug)
        && journal.state_file == state_rel(slug)
        && !journal.before_hash.is_empty()
        && !journal.after_state.revision.is_empty()
}

fn is_json(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("json"))
}

async fn move_failed(dir: &Path, name: &str, raw: &str) {
    let failed = dir.join(format!("failed-{name}"));
    if let Err(err) = crate::storage::write_atomic(&failed, raw.as_bytes(), Some(0o600)).await {
        tracing::warn!("user memory journal {name} could not be set aside: {err}");
        return;
    }
    if let Err(err) = remove_file(&dir.join(name)).await {
        tracing::warn!("user memory journal {name} could not be set aside: {err}");
    }
}
