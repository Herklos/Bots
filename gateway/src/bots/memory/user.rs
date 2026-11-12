//! Person-owned facts outside the Bot folder: `.botapp/memory-user/<slug>.md`
//! and `<slug>.state.json` (revision, per-fact hash, canonical text, at).
//! Writes use `write_atomic` and a journal, never the Workspace trait.

use super::facts::hash_fact;
use super::state::revision_of;
use crate::bots::agent_context::{plain_prompt, MemorySection};
use crate::bots::valid_slug;
use crate::storage::write_atomic;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use uuid::Uuid;

pub(super) const USER_MEMORY_MAX: usize = 2 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Owner {
    User,
    Bot,
}

#[derive(Debug)]
pub(super) struct OwnedFact<'a> {
    pub owner: Owner,
    pub text: &'a str,
    pub at: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PackedBlock {
    pub text: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Packed {
    pub user: PackedBlock,
    pub bot: PackedBlock,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NewFact {
    pub text: String,
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UserFact {
    pub hash: String,
    pub text: String,
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserState {
    version: u32,
    revision: String,
    facts: Vec<UserFact>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JournalFile {
    file: String,
    before_hash: String,
    after: String,
    state_file: String,
    after_state: UserState,
}

/// User block, then bot block. Each owner is newest first. A fact that does not fit is skipped.
pub(super) fn pack(facts: &[OwnedFact<'_>], user_cap: usize, bot_cap: usize) -> Packed {
    Packed {
        user: pack_owner(facts, Owner::User, user_cap),
        bot: pack_owner(facts, Owner::Bot, bot_cap),
    }
}

fn pack_owner(facts: &[OwnedFact<'_>], owner: Owner, cap: usize) -> PackedBlock {
    let mut indexed: Vec<(usize, &OwnedFact<'_>)> = facts
        .iter()
        .enumerate()
        .filter(|(_, fact)| fact.owner == owner)
        .collect();
    indexed.sort_by(|left, right| right.1.at.cmp(left.1.at).then(left.0.cmp(&right.0)));
    let mut text = String::new();
    let mut truncated = false;
    for (_, fact) in indexed {
        let line = plain_prompt(fact.text);
        if line.is_empty() {
            truncated = true;
            continue;
        }
        let extra = line.len() + if text.is_empty() { 0 } else { 1 };
        if text.len() + extra > cap {
            truncated = true;
            continue;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&line);
    }
    PackedBlock { text, truncated }
}

pub(crate) fn user_memory_path(root: &Path, slug: &str) -> PathBuf {
    root.join(".botapp")
        .join("memory-user")
        .join(format!("{slug}.md"))
}

fn journal_dir(root: &Path, slug: &str) -> PathBuf {
    root.join(".botapp")
        .join("memory-user")
        .join(format!("{slug}.journal"))
}

fn file_rel(slug: &str) -> String {
    format!(".botapp/memory-user/{slug}.md")
}

fn state_rel(slug: &str) -> String {
    format!(".botapp/memory-user/{slug}.state.json")
}

fn lock_for(slug: &str) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: OnceLock<std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
        OnceLock::new();
    let locks = LOCKS.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let mut map = locks.lock().unwrap_or_else(|err| err.into_inner());
    map.entry(slug.to_string())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn slug_ok(slug: &str) -> io::Result<()> {
    if valid_slug(slug) {
        Ok(())
    } else {
        Err(invalid("bad slug"))
    }
}

/// Facts for `slug`, newest first when each fact has a time.
/// A missing store is empty. A symlink is an error.
pub(super) async fn load(root: &Path, slug: &str) -> io::Result<Vec<UserFact>> {
    slug_ok(slug)?;
    let _guard = lock_for(slug).lock_owned().await;
    load_locked(root, slug).await
}

async fn load_locked(root: &Path, slug: &str) -> io::Result<Vec<UserFact>> {
    let state_path = checked_path(root, &state_rel(slug)).await?;
    match read_text(&state_path).await? {
        Some(raw) => match serde_json::from_str::<UserState>(&raw) {
            Ok(state) => Ok(state.facts),
            Err(err) => {
                tracing::warn!("user memory state for {slug} was unreadable: {err}");
                facts_from_file(root, slug).await
            }
        },
        None => facts_from_file(root, slug).await,
    }
}

async fn facts_from_file(root: &Path, slug: &str) -> io::Result<Vec<UserFact>> {
    let file_path = checked_path(root, &file_rel(slug)).await?;
    let text = read_text(&file_path).await?.unwrap_or_default();
    Ok(facts_from_lines(&text))
}

fn facts_from_lines(text: &str) -> Vec<UserFact> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| UserFact {
            hash: hash_fact(line),
            text: line.to_string(),
            at: String::new(),
        })
        .collect()
}

/// Replace the person's facts for `slug`. `now` is the journal timestamp.
pub(super) async fn write(root: &Path, slug: &str, facts: &[NewFact], now: &str) -> io::Result<()> {
    slug_ok(slug)?;
    let _guard = lock_for(slug).lock_owned().await;
    let prepared = prepare(facts)?;
    let file_path = checked_path(root, &file_rel(slug)).await?;
    let state_path = checked_path(root, &state_rel(slug)).await?;
    let before_text = read_text(&file_path).await?.unwrap_or_default();
    let before_state = match read_text(&state_path).await? {
        Some(raw) => serde_json::from_str(&raw).unwrap_or_else(|_| blank_state(&before_text)),
        None => blank_state(&before_text),
    };
    let after_text = render_facts(&prepared);
    let after_state = UserState {
        version: 1,
        revision: revision_of(&after_text),
        facts: prepared,
    };
    let op_id = Uuid::new_v4().to_string();
    let journal_rel = format!(".botapp/memory-user/{slug}.journal/{op_id}.json");
    let journal_path = checked_path(root, &journal_rel).await?;
    let body = journal_text(
        &op_id,
        slug,
        &revision_of(&before_text),
        &after_text,
        &before_state,
        &after_state,
        now,
    )?;
    write_atomic(&journal_path, body.as_bytes(), Some(0o600)).await?;
    let wrote = async {
        write_atomic(&file_path, after_text.as_bytes(), Some(0o644)).await?;
        write_state(&state_path, &after_state).await
    }
    .await;
    if wrote.is_ok() {
        if let Err(err) = remove_file(&journal_path).await {
            tracing::warn!("user memory journal {op_id} could not be removed: {err}");
        }
    }
    wrote
}

fn prepare(facts: &[NewFact]) -> io::Result<Vec<UserFact>> {
    let mut indexed = Vec::with_capacity(facts.len());
    for (index, fact) in facts.iter().enumerate() {
        let text = fact.text.trim();
        if text.is_empty() {
            return Err(invalid("a user fact cannot be empty"));
        }
        if text.contains('\n') || text.contains('\r') {
            return Err(invalid("one fact per line"));
        }
        let at = fact.at.trim();
        if at.is_empty() {
            return Err(invalid("a user fact needs a time"));
        }
        indexed.push((
            index,
            UserFact {
                hash: hash_fact(text),
                text: text.to_string(),
                at: at.to_string(),
            },
        ));
    }
    indexed.sort_by(|left, right| right.1.at.cmp(&left.1.at).then(left.0.cmp(&right.0)));
    Ok(indexed.into_iter().map(|(_, fact)| fact).collect())
}

fn render_facts(facts: &[UserFact]) -> String {
    let mut out = String::new();
    for fact in facts {
        out.push_str(&fact.text);
        out.push('\n');
    }
    out
}

fn blank_state(file_text: &str) -> UserState {
    UserState {
        version: 1,
        revision: revision_of(file_text),
        facts: facts_from_lines(file_text),
    }
}

fn journal_text(
    op_id: &str,
    slug: &str,
    before_hash: &str,
    after: &str,
    before_state: &UserState,
    after_state: &UserState,
    now: &str,
) -> io::Result<String> {
    let body = serde_json::json!({
        "version": 1,
        "opId": op_id,
        "file": file_rel(slug),
        "beforeHash": before_hash,
        "after": after,
        "stateFile": state_rel(slug),
        "beforeState": before_state,
        "afterState": after_state,
        "createdAt": now,
    });
    let mut text = serde_json::to_string_pretty(&body)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    text.push('\n');
    Ok(text)
}

async fn write_state(path: &Path, state: &UserState) -> io::Result<()> {
    let mut text = serde_json::to_string_pretty(state)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    text.push('\n');
    write_atomic(path, text.as_bytes(), Some(0o600)).await
}

/// Finish journals left behind for one Bot's user facts.
pub(super) async fn recover(root: &Path, slug: &str) -> io::Result<()> {
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
        write_atomic(&file_path, journal.after.as_bytes(), Some(0o644)).await?;
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
    if let Err(err) = write_atomic(&failed, raw.as_bytes(), Some(0o600)).await {
        tracing::warn!("user memory journal {name} could not be set aside: {err}");
        return;
    }
    if let Err(err) = remove_file(&dir.join(name)).await {
        tracing::warn!("user memory journal {name} could not be set aside: {err}");
    }
}

/// The `<user-memory>` section for a snapshot. Missing, empty, or unreadable is `None`.
pub(crate) async fn snapshot_user(root: &Path, slug: &str) -> Option<MemorySection> {
    if !valid_slug(slug) {
        return None;
    }
    let facts = match load(root, slug).await {
        Ok(facts) => facts,
        Err(err) => {
            tracing::warn!("user memory snapshot for {slug}: {err}");
            return None;
        }
    };
    if facts.is_empty() {
        return None;
    }
    let owned: Vec<OwnedFact<'_>> = facts
        .iter()
        .map(|fact| OwnedFact {
            owner: Owner::User,
            text: &fact.text,
            at: &fact.at,
        })
        .collect();
    let packed = pack(&owned, USER_MEMORY_MAX, USER_MEMORY_MAX);
    if packed.user.text.is_empty() {
        return None;
    }
    Some(MemorySection {
        text: packed.user.text,
        truncated: packed.user.truncated,
        source: file_rel(slug),
    })
}

async fn checked_path(root: &Path, rel: &str) -> io::Result<PathBuf> {
    crate::storage::workspace::safe_rel(rel)?;
    let full = root.join(rel);
    let mut built = root.to_path_buf();
    for part in rel.split('/') {
        built.push(part);
        match tokio::fs::symlink_metadata(&built).await {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(invalid("refusing to follow a symlink"));
            }
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(full),
            Err(err) => return Err(err),
        }
    }
    Ok(full)
}

async fn read_text(path: &Path) -> io::Result<Option<String>> {
    match tokio::fs::symlink_metadata(path).await {
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
        Ok(meta) if meta.file_type().is_symlink() => {
            Err(invalid("refusing to read through a symlink"))
        }
        Ok(meta) if !meta.file_type().is_file() => Err(invalid("not a file")),
        Ok(_) => Ok(Some(tokio::fs::read_to_string(path).await?)),
    }
}

async fn remove_file(path: &Path) -> io::Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}
