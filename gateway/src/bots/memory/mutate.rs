//! One transactional change to a memory file, a shared file, or one note.
use super::evidence::{check_owner_evidence, is_accepted_quote, EvidenceKind, ACCEPTED_VIA};
use super::facts::{
    category_label, check_fact, close_fact_range, format_fact, hash_fact, is_date, local_date,
    parse_fact, MEMORY_CHARS, MEMORY_LINES,
};
use super::state::{self, MemoryState, Provenance, Temporal};
use super::MemoryStore;
use std::io;
use std::path::Path;

mod store;
mod user_rules;

use store::{
    cap, commit, conflict, duplicate, invalid, not_found, paths_for, unsupported, write_note,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    Bot,
    Shared,
}

impl Scope {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Bot => "bot",
            Self::Shared => "shared",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    Add,
    Replace,
    Supersede,
    Note,
    Forget,
}

impl Op {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Replace => "replace",
            Self::Supersede => "supersede",
            Self::Note => "note",
            Self::Forget => "forget",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Mutation<'a> {
    pub slug: &'a str,
    pub op: Op,
    pub scope: Scope,
    pub expected_revision: &'a str,
    pub author: &'a str,
    pub source_message_id: &'a str,
    pub source_quote: &'a str,
    pub source_text: &'a str,
    pub now: &'a str,
    pub restore: bool,
    pub trusted_user: bool,
    pub fact: Option<&'a str>,
    pub old: Option<&'a str>,
    pub new_fact: Option<&'a str>,
    pub category: Option<&'a str>,
    pub path: Option<&'a str>,
    pub description: Option<&'a str>,
    pub content: Option<&'a str>,
    pub valid_from: Option<&'a str>,
    /// Bots root for the person-owned fact file. Absent in stores that have no user file.
    pub user_root: Option<&'a Path>,
    /// Display name used when a rejected edit is recorded. Empty uses the slug.
    pub bot_name: &'a str,
    pub evidence: EvidenceKind,
}

impl<'a> Mutation<'a> {
    pub(crate) fn new(slug: &'a str, op: Op, revision: &'a str) -> Self {
        Self {
            slug,
            op,
            scope: Scope::Bot,
            expected_revision: revision,
            author: slug,
            source_message_id: "m1",
            source_quote: "I prefer short answers",
            source_text: "I prefer short answers, please. Also remember the launch is Thursday.",
            now: "2026-11-09T10:00:00.000Z",
            restore: false,
            trusted_user: false,
            fact: None,
            old: None,
            new_fact: None,
            category: None,
            path: None,
            description: None,
            content: None,
            valid_from: None,
            user_root: None,
            bot_name: "",
            evidence: EvidenceKind::OwnerQuote,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MutateOk {
    pub scope: Scope,
    pub path: String,
    pub revision: String,
    pub line: Option<String>,
    pub forgotten: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug)]
pub(crate) struct Coded {
    pub code: &'static str,
    pub message: String,
    pub revision: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug)]
pub(crate) enum MutateFail {
    Code(Coded),
    Io(io::Error),
}

impl From<io::Error> for MutateFail {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

#[derive(Clone)]
struct Paths {
    lock: String,
    file: String,
    state_file: String,
    history_dir: String,
    journal_dir: String,
    relative: String,
    label: String,
}

pub(crate) async fn mutate(
    store: &MemoryStore,
    input: Mutation<'_>,
) -> Result<MutateOk, MutateFail> {
    let paths = paths_for(input.slug, input.scope);
    let _guard = store.lock_store(&paths.lock).await;
    apply(store, &input, &paths).await
}

async fn apply(
    store: &MemoryStore,
    input: &Mutation<'_>,
    paths: &Paths,
) -> Result<MutateOk, MutateFail> {
    match input.evidence {
        EvidenceKind::Accepted => {
            if !is_accepted_quote(input.source_quote) {
                return Err(invalid(
                    "An accepted suggestion needs evidence that the person accepted it.",
                ));
            }
        }
        EvidenceKind::OwnerQuote => {
            if input.trusted_user {
                if input.author != "user" {
                    return Err(invalid("A trusted memory action must be made by the user."));
                }
            } else {
                check_owner_evidence(input.source_text, input.source_quote)
                    .map_err(|err| invalid(err.message))?;
            }
        }
    }
    if input.author.trim().is_empty() {
        return Err(invalid("A memory write needs an author."));
    }
    if let Some(err) = user_rules::bot_user_refusal(input).await? {
        return Err(err);
    }
    let category = checked_category(input.category)?;
    if input.op == Op::Note && input.scope == Scope::Shared {
        return Err(unsupported(
            "Notes live in a Bot's memory, not in shared memory.",
        ));
    }
    if input.op == Op::Note {
        return write_note(store, input, paths).await;
    }
    let current = store.read_text_or_empty(&paths.file).await?;
    let revision = state::revision_of(&current);
    if revision != input.expected_revision {
        return Err(conflict(
            "Memory changed since you read it. Read it again, merge, and retry with the new revision.",
            &revision,
            &current,
        ));
    }
    let mut memory_state = state::read_state(store, &paths.state_file).await?;
    let (lines, trailing) = split_lines(&current);
    let mut next_lines = lines.clone();
    let mut written = None;
    let mut forgotten = None;
    match input.op {
        Op::Add => add_fact(
            input,
            &category,
            &current,
            &mut next_lines,
            &mut memory_state,
            &mut written,
        )?,
        Op::Replace | Op::Supersede => replace_fact(
            input,
            &category,
            &mut next_lines,
            &mut memory_state,
            &mut written,
        )?,
        Op::Forget => forget_fact(input, &mut next_lines, &mut memory_state, &mut forgotten)?,
        Op::Note => return write_note(store, input, paths).await,
    }
    let next = join_lines(&next_lines, trailing || next_lines.len() > lines.len());
    if nonempty(&next_lines) > MEMORY_LINES {
        return Err(cap(
            "This memory is over its line limit. Move detail into a note or wait for defrag.",
        ));
    }
    if next.chars().count() > MEMORY_CHARS {
        return Err(cap(
            "This memory is over its size limit. Move detail into a note or wait for defrag.",
        ));
    }
    memory_state
        .revisions
        .insert(paths.relative.clone(), state::revision_of(&next));
    commit(store, paths, &current, &next, &memory_state, input.now).await?;
    let saved = state::revision_of(&next);
    tracing::info!(
        slug = input.slug,
        op = input.op.as_str(),
        scope = input.scope.as_str(),
        path = paths.relative,
        "memory write"
    );
    Ok(MutateOk {
        scope: input.scope,
        path: paths.relative.clone(),
        revision: saved,
        line: written,
        forgotten,
        note: None,
    })
}

fn add_fact(
    input: &Mutation<'_>,
    category: &Option<String>,
    current: &str,
    lines: &mut Vec<String>,
    state: &mut MemoryState,
    written: &mut Option<String>,
) -> Result<(), MutateFail> {
    let fact = input.fact.unwrap_or("").trim();
    check_fact(fact).map_err(|err| invalid(err.message))?;
    if let Some(date) = input.valid_from.filter(|date| !date.is_empty()) {
        if !is_date(date) {
            return Err(invalid("validFrom must be a real YYYY-MM-DD date."));
        }
    }
    let key = hash_fact(fact);
    guard_forgotten(&state.forgotten, &key, input.restore)?;
    state::clear_tombstone(&mut state.forgotten, &key);
    if lines
        .iter()
        .any(|line| !line.trim().is_empty() && fact_key(line) == key)
    {
        return Err(duplicate("That fact is already in memory."));
    }
    let line = format_fact(
        fact,
        category.as_deref(),
        Some(&source_label(input)),
        input.valid_from,
    );
    if nonempty(lines) >= MEMORY_LINES
        || current.chars().count() + line.chars().count() + 1 > MEMORY_CHARS
    {
        return Err(cap(
            "This memory is full. Move detail into a note or wait for defrag.",
        ));
    }
    lines.push(line.clone());
    *written = Some(line);
    state.provenance.insert(key.clone(), provenance_of(input));
    if let Some(date) = input.valid_from.filter(|date| !date.is_empty()) {
        state.temporal.insert(
            key,
            Temporal {
                from: Some(date.to_string()),
                to: None,
            },
        );
    }
    Ok(())
}

fn replace_fact(
    input: &Mutation<'_>,
    category: &Option<String>,
    lines: &mut Vec<String>,
    state: &mut MemoryState,
    written: &mut Option<String>,
) -> Result<(), MutateFail> {
    let index = find_line(lines, input.old)?;
    let old_line = lines[index].clone();
    let fact = input.new_fact.unwrap_or("").trim();
    check_fact(fact).map_err(|err| invalid(err.message))?;
    let key = hash_fact(fact);
    guard_forgotten(&state.forgotten, &key, input.restore)?;
    state::clear_tombstone(&mut state.forgotten, &key);
    let old = parse_fact(&old_line);
    let valid_from = input
        .valid_from
        .filter(|date| !date.is_empty())
        .map(str::to_string)
        .or_else(|| {
            if input.op == Op::Supersede {
                Some(local_date(input.now))
            } else {
                old.valid_from.clone()
            }
        });
    if input.op == Op::Supersede && valid_from.as_deref().unwrap_or("").is_empty() {
        return Err(invalid("A superseding fact needs a validFrom date."));
    }
    if let Some(date) = &valid_from {
        if !is_date(date) {
            return Err(invalid("validFrom must be a real YYYY-MM-DD date."));
        }
    }
    let label = category.clone().or(old.category);
    let line = format_fact(
        fact,
        label.as_deref(),
        Some(&source_label(input)),
        valid_from.as_deref(),
    );
    if input.op == Op::Supersede {
        let closed = close_fact_range(&old_line, valid_from.as_deref().unwrap_or(""));
        lines.splice(index..=index, [closed, line.clone()]);
        state.temporal.insert(
            fact_key(&old_line),
            Temporal {
                from: old.valid_from,
                to: valid_from.clone(),
            },
        );
        state.temporal.insert(
            key.clone(),
            Temporal {
                from: valid_from,
                to: None,
            },
        );
    } else {
        lines[index] = line.clone();
    }
    *written = Some(line);
    state.provenance.insert(key, provenance_of(input));
    Ok(())
}

fn forget_fact(
    input: &Mutation<'_>,
    lines: &mut Vec<String>,
    state: &mut MemoryState,
    forgotten: &mut Option<String>,
) -> Result<(), MutateFail> {
    let wanted = input.fact.or(input.old);
    let index = find_line(lines, wanted)?;
    let key = fact_key(&lines[index]);
    lines.remove(index);
    state::tombstone(&mut state.forgotten, &key);
    *forgotten = Some(key);
    Ok(())
}

fn checked_category(value: Option<&str>) -> Result<Option<String>, MutateFail> {
    match value.map(str::trim).filter(|label| !label.is_empty()) {
        None => Ok(None),
        Some(raw) => category_label(raw).map(Some).ok_or_else(|| {
            invalid("category must be a short lowercase label (letters, digits, - or _).")
        }),
    }
}

fn find_line(lines: &[String], value: Option<&str>) -> Result<usize, MutateFail> {
    let wanted = value.unwrap_or("").trim();
    if wanted.is_empty() {
        return Err(invalid("Pass the exact existing fact in old."));
    }
    let matches: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim() == wanted)
        .map(|(index, _)| index)
        .collect();
    match matches.as_slice() {
        [index] => Ok(*index),
        [] => Err(not_found(
            "That fact isn't in this memory file. Read it again before replacing or forgetting it.",
        )),
        _ => Err(invalid(
            "That fact appears more than once. Read the file and remove the duplicate first.",
        )),
    }
}

fn guard_forgotten(forgotten: &[String], key: &str, restore: bool) -> Result<(), MutateFail> {
    if forgotten.iter().any(|item| item == key) && !restore {
        return Err(MutateFail::Code(Coded {
            code: "forgotten",
            message: "The user forgot that fact. Only a new, explicit request from the user can bring it back; pass restore: true for that request.".into(),
            revision: None,
            content: None,
        }));
    }
    Ok(())
}

fn split_lines(content: &str) -> (Vec<String>, bool) {
    let trailing = content.ends_with('\n');
    let normalized = super::evidence::normalize_newlines(content);
    let mut lines: Vec<String> = normalized.split('\n').map(str::to_string).collect();
    if trailing {
        lines.pop();
    }
    (lines, trailing)
}

fn join_lines(lines: &[String], trailing: bool) -> String {
    let mut text = lines.join("\n");
    if trailing {
        text.push('\n');
    }
    text
}

fn nonempty(lines: &[String]) -> usize {
    lines.iter().filter(|line| !line.trim().is_empty()).count()
}

fn fact_key(line: &str) -> String {
    hash_fact(&parse_fact(line).text)
}

fn source_label(input: &Mutation<'_>) -> String {
    format!(
        "thread/{}; added: {}",
        input.source_message_id,
        local_date(input.now)
    )
}

fn provenance_of(input: &Mutation<'_>) -> Provenance {
    Provenance {
        author: input.author.to_string(),
        source_message_id: Some(input.source_message_id.to_string()),
        quote: Some(input.source_quote.to_string()),
        at: input.now.to_string(),
        via: match input.evidence {
            EvidenceKind::OwnerQuote => None,
            EvidenceKind::Accepted => Some(ACCEPTED_VIA.to_string()),
        },
    }
}
