//! Search, the note index, and the view both layers share.
use super::evidence::evidence_text;
#[cfg(test)]
use super::facts;
use super::facts::{parse_fact, NOTE_COUNT, NOTE_DEPTH, NOTE_TOKENS};
use super::state;
#[cfg(test)]
use super::state::{MemoryState, Provenance};
use super::MemoryStore;
use crate::storage::workspace::EntryKind;
use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::HashSet;
use std::io;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hit {
    pub scope: &'static str,
    pub path: String,
    pub line: usize,
    pub text: String,
    pub score: i32,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoteInfo {
    pub path: String,
    pub description: String,
    pub chars: usize,
    pub tokens: usize,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoteBody {
    pub path: String,
    pub text: String,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Document {
    pub text: String,
    pub revision: String,
    pub truncated: bool,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FactRow {
    pub category: Option<String>,
    pub text: String,
    pub provenance: Option<Provenance>,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Layer {
    pub revision: String,
    pub facts: Vec<FactRow>,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MemoryView {
    pub bot: Layer,
    pub shared: Layer,
    pub forgotten: usize,
}

pub(super) fn normalize_note_path(note_path: &str) -> Result<String, String> {
    let normalized = note_path.replace('\\', "/");
    let normalized = normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .to_string();
    if !normalized.starts_with("memory/notes/")
        || !is_md(&normalized)
        || normalized.contains("..")
        || normalized.contains('\0')
    {
        return Err("A note path must be under memory/notes and end in .md".into());
    }
    let segments: Vec<&str> = normalized.split('/').collect();
    if segments.iter().any(|segment| segment.is_empty()) {
        return Err("A note path has an empty segment".into());
    }
    if segments.len() > 5 {
        return Err("A note path may have at most two folder levels under memory/notes".into());
    }
    Ok(normalized)
}

pub(crate) async fn list_notes(store: &MemoryStore, slug: &str) -> io::Result<Vec<NoteInfo>> {
    let mut notes = Vec::new();
    walk(store, &format!("{slug}/memory/notes"), "", 0, &mut notes).await?;
    notes.retain(|note| {
        !note.path.starts_with("memory/notes/defrag/")
            && !note.path.starts_with("memory/notes/history/")
            && !note.path.starts_with("memory/notes/archive/")
    });
    notes.sort_by(|a, b| a.path.cmp(&b.path));
    notes.truncate(NOTE_COUNT);
    Ok(notes)
}

pub(crate) async fn read_note(
    store: &MemoryStore,
    slug: &str,
    path: &str,
    max_chars: usize,
) -> Result<NoteBody, String> {
    let normalized = normalize_note_path(path)?;
    let rel = format!("{slug}/{normalized}");
    if crate::storage::workspace::safe_rel(&rel).is_err()
        || store.refuse_symlink(&rel).await.is_err()
    {
        return Err("That note is outside this Bot's memory.".into());
    }
    let full = match store.read_text(&rel).await {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Err("That note is missing.".into());
        }
        Err(err) => return Err(err.to_string()),
    };
    let (text, _) = clip(
        &full,
        max_chars,
        "(truncated; read the note on disk for the rest)",
    );
    Ok(NoteBody {
        path: normalized,
        text,
        revision: state::revision_of(&full),
    })
}

pub(crate) async fn document(
    store: &MemoryStore,
    rel: &str,
    max_chars: usize,
) -> io::Result<Document> {
    let raw = match store.refuse_symlink(rel).await {
        Ok(()) => match store.read_text(rel).await {
            Ok(text) => text,
            Err(err)
                if err.kind() == io::ErrorKind::NotFound
                    || err.kind() == io::ErrorKind::InvalidInput =>
            {
                String::new()
            }
            Err(err) => return Err(err),
        },
        Err(err) if err.kind() == io::ErrorKind::InvalidInput => String::new(),
        Err(err) => return Err(err),
    };
    let (text, truncated) = clip(
        &raw,
        max_chars,
        "(truncated; read the file on disk for the rest)",
    );
    Ok(Document {
        text,
        revision: state::revision_of(&raw),
        truncated,
    })
}

#[cfg(test)]
pub(super) async fn read_view(store: &MemoryStore, slug: &str) -> io::Result<MemoryView> {
    let bot = document(store, &MemoryStore::bot_memory(slug), facts::MEMORY_CHARS).await?;
    let shared = document(store, MemoryStore::shared_memory(), facts::MEMORY_CHARS).await?;
    let bot_state = state::read_state(store, &MemoryStore::bot_state(slug)).await?;
    let shared_state = state::read_state(store, MemoryStore::shared_state()).await?;
    let mut forgotten = HashSet::new();
    forgotten.extend(bot_state.forgotten.iter().cloned());
    forgotten.extend(shared_state.forgotten.iter().cloned());
    Ok(MemoryView {
        bot: layer(&bot, &bot_state),
        shared: layer(&shared, &shared_state),
        forgotten: forgotten.len(),
    })
}

pub(crate) async fn search(
    store: &MemoryStore,
    slug: &str,
    query: &str,
    limit: usize,
    include_shared: bool,
) -> io::Result<Vec<Hit>> {
    let terms: Vec<String> = evidence_text(query)
        .split(' ')
        .filter(|term| !term.is_empty())
        .map(str::to_string)
        .collect();
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let phrase = terms.join(" ");
    let mut files = Vec::new();
    push_file(
        &mut files,
        "bot",
        "MEMORY.md",
        &read_if_present(store, &MemoryStore::bot_memory(slug)).await?,
    );
    if include_shared {
        push_file(
            &mut files,
            "shared",
            "shared-memory.md",
            &read_if_present(store, MemoryStore::shared_memory()).await?,
        );
    }
    for note in list_notes(store, slug).await? {
        push_file(
            &mut files,
            "bot",
            &note.path,
            &read_if_present(store, &format!("{slug}/{}", note.path)).await?,
        );
    }
    let mut hits = Vec::new();
    for (scope, path, text) in files {
        for (index, raw) in text.split('\n').enumerate() {
            if raw.trim().is_empty() {
                continue;
            }
            let normalized = evidence_text(raw);
            let exact = normalized.contains(&phrase);
            let all = terms.iter().all(|term| normalized.contains(term.as_str()));
            let any = terms.iter().any(|term| normalized.contains(term.as_str()));
            if !exact && !all && !any {
                continue;
            }
            let score = if exact {
                10
            } else if all {
                5
            } else {
                2
            };
            hits.push(Hit {
                scope,
                path: path.clone(),
                line: index + 1,
                text: raw.trim().to_string(),
                score,
                source: parse_fact(raw).source,
            });
        }
    }
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.line.cmp(&b.line))
    });
    hits.truncate(limit.max(1));
    Ok(hits)
}

#[cfg(test)]
fn layer(document: &Document, state: &MemoryState) -> Layer {
    let facts = document
        .text
        .split('\n')
        .filter(|line| facts::is_bullet(line))
        .map(|raw| {
            let parsed = parse_fact(raw);
            let hash = facts::hash_fact(&parsed.text);
            FactRow {
                category: parsed.category,
                text: parsed.text,
                provenance: state.provenance.get(&hash).cloned(),
            }
        })
        .collect();
    Layer {
        revision: document.revision.clone(),
        facts,
    }
}

fn push_file(
    files: &mut Vec<(&'static str, String, String)>,
    scope: &'static str,
    path: &str,
    text: &str,
) {
    if !text.is_empty() {
        files.push((scope, path.to_string(), text.to_string()));
    }
}

async fn read_if_present(store: &MemoryStore, rel: &str) -> io::Result<String> {
    if store.refuse_symlink(rel).await.is_err() {
        return Ok(String::new());
    }
    match store.read_text_or_empty(rel).await {
        Ok(text) => Ok(text),
        Err(err) if err.kind() == io::ErrorKind::InvalidInput => Ok(String::new()),
        Err(err) => Err(err),
    }
}

async fn walk(
    store: &MemoryStore,
    dir: &str,
    under: &str,
    depth: usize,
    out: &mut Vec<NoteInfo>,
) -> io::Result<()> {
    if depth > NOTE_DEPTH {
        return Ok(());
    }
    let entries = match store.ws.list(dir).await {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };
    for entry in entries {
        if entry.name.starts_with('.') || entry.name.contains('/') {
            continue;
        }
        let rel = format!("{dir}/{}", entry.name);
        let kind = store.ws.symlink_kind(&rel).await?;
        if kind == Some(EntryKind::Dir) {
            if depth < NOTE_DEPTH {
                let nested = if under.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{under}/{}", entry.name)
                };
                Box::pin(walk(store, &rel, &nested, depth + 1, out)).await?;
            }
            continue;
        }
        if kind != Some(EntryKind::File) || !is_md(&entry.name) {
            continue;
        }
        let text = read_if_present(store, &rel).await?;
        let path = if under.is_empty() {
            format!("memory/notes/{}", entry.name)
        } else {
            format!("memory/notes/{under}/{}", entry.name)
        };
        out.push(NoteInfo {
            path,
            description: note_description(&entry.name, &text),
            chars: text.chars().count(),
            tokens: estimate_tokens(&text),
            updated_at: (entry.modified > 0).then(|| {
                let secs = i64::try_from(entry.modified).unwrap_or(i64::MAX);
                crate::vault::iso(secs.saturating_mul(1000))
            }),
        });
    }
    Ok(())
}

fn clip(raw: &str, max_chars: usize, note: &str) -> (String, bool) {
    if raw.chars().count() <= max_chars {
        return (raw.to_string(), false);
    }
    let text: String = raw.chars().take(max_chars).collect();
    (format!("{}\n\n{note}", text.trim_end()), true)
}

fn note_description(name: &str, text: &str) -> String {
    let front = frontmatter(text);
    let raw = front
        .get("description")
        .cloned()
        .or_else(|| front.get("title").cloned())
        .or_else(|| heading(text))
        .unwrap_or_else(|| stem(name));
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn frontmatter(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(rest) = text.strip_prefix("---") else {
        return out;
    };
    let Some(end) = rest.find("\n---") else {
        return out;
    };
    for line in rest[..end].lines() {
        let Some((key, value)) = line.trim().split_once(':') else {
            continue;
        };
        if !key.starts_with(|ch: char| ch.is_ascii_alphabetic()) {
            continue;
        }
        if !key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        {
            continue;
        }
        let mut value = value.trim().to_string();
        if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            value = value[1..value.len() - 1].to_string();
        }
        out.insert(key.to_lowercase(), value);
    }
    out
}

fn heading(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("# ")
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_string)
    })
}

fn estimate_tokens(text: &str) -> usize {
    let len = text.chars().count();
    let tokens = if len == 0 { 1 } else { len.div_ceil(4) };
    tokens.clamp(1, NOTE_TOKENS)
}

fn is_md(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("md"))
}

fn stem(name: &str) -> String {
    name.rsplit_once('.')
        .filter(|(_, ext)| ext.eq_ignore_ascii_case("md"))
        .map(|(stem, _)| stem.to_string())
        .unwrap_or_else(|| name.to_string())
}
