//! Where a mutation lands, and the journal around the write.
use super::super::facts::{check_fact, check_note, NOTE_COUNT};
use super::super::search::{self, normalize_note_path};
use super::super::state::{self, MemoryState};
use super::super::{history, journal, MemoryStore};
use super::{provenance_of, Coded, MutateFail, MutateOk, Mutation, Paths, Scope};
use std::io;

pub(super) async fn write_note(
    store: &MemoryStore,
    input: &Mutation<'_>,
    paths: &Paths,
) -> Result<MutateOk, MutateFail> {
    let normalized = normalize_note_path(input.path.unwrap_or("")).map_err(path)?;
    let description = input.description.unwrap_or("").replace(['\r', '\n'], " ");
    let description = description.replace('"', "'").trim().to_string();
    if description.is_empty() || description.chars().count() > 160 {
        return Err(invalid("Pass a description of 1 to 160 characters."));
    }
    check_fact(&description).map_err(|err| invalid(format!("description: {}", err.message)))?;
    let body = input.content.unwrap_or("");
    if body.trim().is_empty() {
        return Err(invalid("Pass the note content."));
    }
    let rel = format!("{}/{normalized}", input.slug);
    if crate::storage::workspace::safe_rel(&rel).is_err() {
        return Err(path("That note is outside this Bot's memory."));
    }
    let existing = store.read_text_or_empty(&rel).await?;
    let revision = state::revision_of(&existing);
    if existing.is_empty() && search::list_notes(store, input.slug).await?.len() >= NOTE_COUNT {
        return Err(cap(format!(
            "This Bot already has {NOTE_COUNT} notes. Merge or remove one first."
        )));
    }
    if revision != input.expected_revision
        && !(existing.is_empty() && input.expected_revision.is_empty())
    {
        return Err(conflict(
            "The note changed since you read it. Read it again and merge.",
            &revision,
            &existing,
        ));
    }
    let title = stem(normalized.rsplit('/').next().unwrap_or("note"));
    let title = title.replace('"', "'");
    let with_frontmatter = if body.starts_with("---") {
        body.to_string()
    } else {
        format!("---\ndescription: {description}\ntitle: {title}\ntype: note\n---\n{body}")
    };
    check_note(&with_frontmatter).map_err(|err| invalid(err.message))?;
    let mut memory_state = state::read_state(store, &paths.state_file).await?;
    memory_state
        .revisions
        .insert(normalized.clone(), state::revision_of(&with_frontmatter));
    memory_state
        .provenance
        .insert(format!("note:{normalized}"), provenance_of(input));
    let mut note_paths = paths.clone();
    note_paths.label = format!("note-{title}");
    note_paths.file = rel;
    note_paths.relative = normalized.clone();
    commit(
        store,
        &note_paths,
        &existing,
        &with_frontmatter,
        &memory_state,
        input.now,
    )
    .await?;
    let saved = state::revision_of(&with_frontmatter);
    tracing::info!(
        slug = input.slug,
        op = "note",
        scope = input.scope.as_str(),
        path = normalized,
        "memory write"
    );
    Ok(MutateOk {
        scope: input.scope,
        path: normalized,
        revision: saved,
        line: None,
        forgotten: None,
        note: Some(with_frontmatter),
    })
}

pub(super) async fn commit(
    store: &MemoryStore,
    paths: &Paths,
    before: &str,
    after: &str,
    after_state: &MemoryState,
    now: &str,
) -> Result<(), MutateFail> {
    let before_state = state::read_state(store, &paths.state_file).await?;
    if let Err(err) =
        history::write_preimage(store, &paths.history_dir, &paths.label, before, now).await
    {
        tracing::warn!("writing memory history failed: {err}");
    }
    let op_id = journal::begin(
        store,
        &paths.journal_dir,
        &journal::Pending {
            file: &paths.file,
            before,
            after,
            state_file: &paths.state_file,
            before_state: &before_state,
            after_state,
            now,
        },
    )
    .await?;
    let wrote = async {
        store.write_text(&paths.file, after, Some(0o644)).await?;
        state::write_state(store, &paths.state_file, after_state).await?;
        Ok::<(), io::Error>(())
    }
    .await;
    journal::finish(store, &paths.journal_dir, &op_id).await;
    wrote?;
    Ok(())
}

pub(super) fn paths_for(slug: &str, scope: Scope) -> Paths {
    match scope {
        Scope::Shared => Paths {
            lock: "shared".into(),
            file: MemoryStore::shared_memory().into(),
            state_file: MemoryStore::shared_state().into(),
            history_dir: MemoryStore::shared_history_dir().into(),
            journal_dir: MemoryStore::shared_journal_dir().into(),
            relative: "shared-memory.md".into(),
            label: "shared-memory".into(),
        },
        Scope::Bot => Paths {
            lock: format!("bot:{slug}"),
            file: MemoryStore::bot_memory(slug),
            state_file: MemoryStore::bot_state(slug),
            history_dir: MemoryStore::bot_history_dir(slug),
            journal_dir: MemoryStore::bot_journal_dir(slug),
            relative: "MEMORY.md".into(),
            label: "MEMORY".into(),
        },
    }
}

pub(super) fn invalid(message: impl Into<String>) -> MutateFail {
    code("invalid", message)
}

pub(super) fn duplicate(message: impl Into<String>) -> MutateFail {
    code("duplicate", message)
}

pub(super) fn not_found(message: impl Into<String>) -> MutateFail {
    code("not_found", message)
}

pub(super) fn cap(message: impl Into<String>) -> MutateFail {
    code("cap", message)
}

pub(super) fn path(message: impl Into<String>) -> MutateFail {
    code("path", message)
}

pub(super) fn unsupported(message: impl Into<String>) -> MutateFail {
    code("unsupported", message)
}

pub(crate) fn refusal(which: &'static str, message: impl Into<String>) -> MutateFail {
    code(which, message)
}

pub(super) fn conflict(message: &str, revision: &str, content: &str) -> MutateFail {
    MutateFail::Code(Coded {
        code: "conflict",
        message: message.to_string(),
        revision: Some(revision.to_string()),
        content: Some(content.to_string()),
    })
}

fn code(code: &'static str, message: impl Into<String>) -> MutateFail {
    MutateFail::Code(Coded {
        code,
        message: message.into(),
        revision: None,
        content: None,
    })
}

fn stem(name: &str) -> String {
    name.rsplit_once('.')
        .filter(|(_, ext)| ext.eq_ignore_ascii_case("md"))
        .map(|(stem, _)| stem.to_string())
        .unwrap_or_else(|| name.to_string())
}
