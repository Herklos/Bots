//! A journal is written before the files and removed after they land.
//! A crash in between is what [`recover`] finishes. A refused journal is kept and not retried.
use super::state::{self, MemoryState};
use super::MemoryStore;
use serde::Deserialize;
use serde_json::Value;
use std::io;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JournalFile {
    #[serde(default)]
    op_id: String,
    file: String,
    before_hash: String,
    after: String,
    state_file: String,
    after_state: Value,
}

#[derive(Debug)]
pub(super) struct Pending<'a> {
    pub file: &'a str,
    pub before: &'a str,
    pub after: &'a str,
    pub state_file: &'a str,
    pub before_state: &'a MemoryState,
    pub after_state: &'a MemoryState,
    pub now: &'a str,
}

pub(super) async fn begin(
    store: &MemoryStore,
    dir: &str,
    pending: &Pending<'_>,
) -> io::Result<String> {
    let op_id = uuid::Uuid::new_v4().to_string();
    let before_state = serde_json::to_value(pending.before_state)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let after_state = serde_json::to_value(pending.after_state)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let body = serde_json::json!({
        "version": 1,
        "opId": op_id,
        "file": pending.file,
        "beforeHash": state::revision_of(pending.before),
        "after": pending.after,
        "stateFile": pending.state_file,
        "beforeState": before_state,
        "afterState": after_state,
        "createdAt": pending.now,
    });
    let mut text = serde_json::to_string_pretty(&body)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    text.push('\n');
    store
        .write_text(&format!("{dir}/{op_id}.json"), &text, Some(0o600))
        .await?;
    Ok(op_id)
}

pub(super) async fn finish(store: &MemoryStore, dir: &str, op_id: &str) {
    if let Err(err) = store.ws.remove(&format!("{dir}/{op_id}.json")).await {
        tracing::warn!("memory journal {op_id} could not be removed: {err}");
    }
}

/// Finish journals left behind for one Bot and for the shared store.
pub(super) async fn recover(store: &MemoryStore, slug: &str) -> io::Result<()> {
    if crate::bots::valid_slug(slug) {
        let prefix = format!("{slug}/memory/");
        recover_dir(store, &MemoryStore::bot_journal_dir(slug), &prefix).await?;
    }
    recover_dir(store, MemoryStore::shared_journal_dir(), ".botapp/").await
}

async fn recover_dir(store: &MemoryStore, dir: &str, prefix: &str) -> io::Result<()> {
    let entries = match store.ws.list(dir).await {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };
    let names: Vec<String> = entries
        .into_iter()
        .map(|entry| entry.name)
        .filter(|name| is_json(name) && !name.starts_with("failed-"))
        .collect();
    for name in names {
        let path = format!("{dir}/{name}");
        let raw = match store.read_text(&path).await {
            Ok(text) => text,
            Err(err) => {
                tracing::warn!("memory journal {name} could not be read: {err}");
                continue;
            }
        };
        if let Err(err) = apply_one(store, dir, &name, &raw, prefix).await {
            tracing::warn!("memory journal {name} was not applied: {err}");
        }
    }
    Ok(())
}

async fn apply_one(
    store: &MemoryStore,
    dir: &str,
    name: &str,
    raw: &str,
    prefix: &str,
) -> io::Result<()> {
    let journal = match serde_json::from_str::<JournalFile>(raw) {
        Ok(journal)
            if !journal.file.is_empty()
                && !journal.state_file.is_empty()
                && !journal.before_hash.is_empty()
                && journal.after_state.is_object() =>
        {
            journal
        }
        _ => {
            move_failed(store, dir, name, raw).await;
            tracing::info!("memory journal {name} is unreadable and was kept for review");
            return Ok(());
        }
    };
    if !allowed(&journal.file, prefix) || !allowed(&journal.state_file, prefix) {
        move_failed(store, dir, name, raw).await;
        tracing::info!(
            op = %journal.op_id,
            "memory journal refused: target outside the memory folder"
        );
        return Ok(());
    }
    let current = store.read_text_or_empty(&journal.file).await?;
    let current_hash = state::revision_of(&current);
    let after_hash = state::revision_of(&journal.after);
    if current_hash == journal.before_hash {
        store
            .write_text(&journal.file, &journal.after, Some(0o644))
            .await?;
        write_after_state(store, &journal).await?;
        store.ws.remove(&format!("{dir}/{name}")).await?;
        tracing::info!(file = %journal.file, op = %journal.op_id, "memory journal recovered");
    } else if current_hash == after_hash {
        write_after_state(store, &journal).await?;
        store.ws.remove(&format!("{dir}/{name}")).await?;
        tracing::info!(file = %journal.file, op = %journal.op_id, "memory journal completed state");
    } else {
        move_failed(store, dir, name, raw).await;
        tracing::info!(
            file = %journal.file,
            op = %journal.op_id,
            "memory journal refused: the file changed outside the store"
        );
    }
    Ok(())
}

fn is_json(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("json"))
}

fn allowed(key: &str, prefix: &str) -> bool {
    key.starts_with(prefix)
        && !key.contains("..")
        && crate::storage::workspace::safe_rel(key).is_ok()
}

async fn write_after_state(store: &MemoryStore, journal: &JournalFile) -> io::Result<()> {
    let text = serde_json::to_string(&journal.after_state)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let state = state::parse_state(&text);
    state::write_state(store, &journal.state_file, &state).await
}

async fn move_failed(store: &MemoryStore, dir: &str, name: &str, raw: &str) {
    let failed = format!("{dir}/failed-{name}");
    if let Err(err) = store.write_text(&failed, raw, Some(0o600)).await {
        tracing::warn!("memory journal {name} could not be set aside: {err}");
        return;
    }
    if let Err(err) = store.ws.remove(&format!("{dir}/{name}")).await {
        tracing::warn!("memory journal {name} could not be set aside: {err}");
    }
}
