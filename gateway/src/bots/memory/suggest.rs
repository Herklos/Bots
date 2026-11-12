//! A Bot proposes a fact. The person accepts or dismisses it.
//! The file is `.botapp/memory-suggestions/<slug>.json`, never `MEMORY.md`.

use super::evidence::{accepted_quote, EvidenceKind};
use super::facts::{category_label, check_fact, hash_fact, is_date, parse_fact, strip_source};
use super::state::revision_of;
use super::{mutate, MemoryStore, MutateFail, MutateOk, Mutation, Op};
use crate::bots::valid_slug;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;

const PENDING_CAP: usize = 20;
const WHY_CHARS: usize = 300;
const DISMISS_DAYS: i64 = 90;

#[derive(Debug)]
pub(crate) struct SuggestFail {
    pub code: &'static str,
    pub message: String,
}

impl SuggestFail {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<io::Error> for SuggestFail {
    fn from(err: io::Error) -> Self {
        Self::new("io", err.to_string())
    }
}

impl From<MutateFail> for SuggestFail {
    fn from(err: MutateFail) -> Self {
        match err {
            MutateFail::Code(coded) => Self::new(coded.code, coded.message),
            MutateFail::Io(err) => Self::new("io", err.to_string()),
        }
    }
}

pub(crate) struct Proposal<'a> {
    pub slug: &'a str,
    pub text: &'a str,
    pub why: &'a str,
    pub category: Option<&'a str>,
    pub now: &'a str,
    pub user_root: Option<&'a Path>,
}

#[derive(Debug)]
pub(crate) enum ProposalResult {
    Stored { id: String, pending: usize },
    Dropped,
}

#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the Memory tab accepts and dismisses in a later phase; tests call this now"
    )
)]
pub(crate) struct Acceptance<'a> {
    pub slug: &'a str,
    pub id: &'a str,
    /// When the person edits the text before accepting, this is what is stored.
    pub edited: Option<&'a str>,
    pub now: &'a str,
    pub user_root: Option<&'a Path>,
    pub bot_name: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct SuggestionFile {
    #[serde(default = "file_version")]
    version: u32,
    #[serde(default)]
    pending: Vec<Pending>,
    #[serde(default)]
    dismissed: Vec<Dismissed>,
}

fn file_version() -> u32 {
    1
}

impl SuggestionFile {
    fn empty() -> Self {
        Self {
            version: 1,
            pending: Vec::new(),
            dismissed: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Pending {
    id: String,
    text: String,
    why: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    hash: String,
    at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Dismissed {
    hash: String,
    at: String,
}

pub(crate) async fn propose(
    store: &MemoryStore,
    input: &Proposal<'_>,
) -> Result<ProposalResult, SuggestFail> {
    let slug = checked_slug(input.slug)?;
    let text = checked_text(input.text)?;
    let why = checked_why(input.why)?;
    let category = checked_category(input.category)?;
    let _guard = store.lock_store(&lock_key(&slug)).await;
    let mut file = read_file(store, &slug).await?;
    let pruned = prune(&mut file, input.now);
    let hash = hash_fact(&text);
    if duplicate(&file, &hash)
        || known_hashes(store, &slug, input.user_root)
            .await?
            .iter()
            .any(|known| known == &hash)
    {
        if pruned {
            write_file(store, &slug, &file).await?;
        }
        return Ok(ProposalResult::Dropped);
    }
    if file.pending.len() >= PENDING_CAP {
        if pruned {
            write_file(store, &slug, &file).await?;
        }
        return Err(SuggestFail::new(
            "cap",
            format!(
                "There are already {PENDING_CAP} suggestions waiting. Accept or dismiss one before proposing another."
            ),
        ));
    }
    let id = suggestion_id(&hash, &file.pending);
    file.pending.push(Pending {
        id: id.clone(),
        text,
        why,
        category,
        hash,
        at: input.now.to_string(),
    });
    let pending = file.pending.len();
    write_file(store, &slug, &file).await?;
    tracing::info!(slug, id, "memory suggestion stored");
    Ok(ProposalResult::Stored { id, pending })
}

#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the Memory tab accepts and dismisses in a later phase; tests call this now"
    )
)]
pub(crate) async fn accept(
    store: &MemoryStore,
    input: &Acceptance<'_>,
) -> Result<MutateOk, SuggestFail> {
    let slug = checked_slug(input.slug)?;
    let _guard = store.lock_store(&lock_key(&slug)).await;
    let mut file = read_file(store, &slug).await?;
    let Some(index) = file.pending.iter().position(|item| item.id == input.id) else {
        return Err(SuggestFail::new(
            "not_found",
            "That suggestion is not waiting.",
        ));
    };
    let item = file.pending[index].clone();
    let text = accepted_text(input.edited, &item.text)?;
    let current = store
        .read_text_or_empty(&MemoryStore::bot_memory(&slug))
        .await?;
    let revision = revision_of(&current);
    let quote = accepted_quote(input.now);
    let name = if input.bot_name.trim().is_empty() {
        slug.clone()
    } else {
        input.bot_name.to_string()
    };
    let mut mutation = Mutation::new(&slug, Op::Add, &revision);
    mutation.evidence = EvidenceKind::Accepted;
    mutation.author = &slug;
    mutation.source_message_id = input.id;
    mutation.source_quote = &quote;
    mutation.source_text = &quote;
    mutation.now = input.now;
    mutation.fact = Some(&text);
    mutation.category = item.category.as_deref();
    mutation.user_root = input.user_root;
    mutation.bot_name = &name;
    let saved = mutate(store, mutation).await?;
    file.pending.remove(index);
    write_file(store, &slug, &file).await?;
    tracing::info!(slug, id = input.id, "memory suggestion accepted");
    Ok(saved)
}

#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the Memory tab accepts and dismisses in a later phase; tests call this now"
    )
)]
pub(crate) async fn dismiss(
    store: &MemoryStore,
    slug: &str,
    id: &str,
    now: &str,
) -> Result<(), SuggestFail> {
    let slug = checked_slug(slug)?;
    let _guard = store.lock_store(&lock_key(&slug)).await;
    let mut file = read_file(store, &slug).await?;
    let Some(index) = file.pending.iter().position(|item| item.id == id) else {
        return Err(SuggestFail::new(
            "not_found",
            "That suggestion is not waiting.",
        ));
    };
    let item = file.pending.remove(index);
    file.dismissed.retain(|old| old.hash != item.hash);
    file.dismissed.push(Dismissed {
        hash: item.hash,
        at: now.to_string(),
    });
    prune(&mut file, now);
    write_file(store, &slug, &file).await?;
    tracing::info!(slug, id, "memory suggestion dismissed");
    Ok(())
}

fn accepted_text(edited: Option<&str>, stored: &str) -> Result<String, SuggestFail> {
    let text = match edited {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Err(SuggestFail::new(
                    "invalid",
                    "The edited fact cannot be empty.",
                ));
            }
            checked_text(trimmed)?
        }
        None => stored.to_string(),
    };
    Ok(text)
}

fn duplicate(file: &SuggestionFile, hash: &str) -> bool {
    file.pending.iter().any(|item| item.hash == hash)
        || file.dismissed.iter().any(|item| item.hash == hash)
}

async fn known_hashes(
    store: &MemoryStore,
    slug: &str,
    user_root: Option<&Path>,
) -> Result<Vec<String>, SuggestFail> {
    let mut hashes = Vec::new();
    for rel in [
        MemoryStore::bot_memory(slug),
        MemoryStore::shared_memory().to_string(),
    ] {
        let text = store.read_text_or_empty(&rel).await?;
        hashes.extend(bullet_hashes(&text));
    }
    if let Some(root) = user_root {
        let facts = super::user::load(root, slug).await?;
        hashes.extend(facts.into_iter().map(|fact| fact.hash));
    }
    Ok(hashes)
}

fn bullet_hashes(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| line.starts_with("- ") || line.starts_with("* ") || line.starts_with("+ "))
        .filter_map(|line| {
            let text = parse_fact(line).text;
            if text.is_empty() {
                None
            } else {
                Some(hash_fact(&text))
            }
        })
        .collect()
}

fn suggestion_id(hash: &str, pending: &[Pending]) -> String {
    let mut width = 6.min(hash.len());
    loop {
        let id = format!("s_{}", &hash[..width]);
        let clash = pending
            .iter()
            .any(|item| item.id == id && item.hash != hash);
        if !clash {
            return id;
        }
        if width == hash.len() {
            return format!("s_{hash}");
        }
        width = (width + 2).min(hash.len());
    }
}

fn prune(file: &mut SuggestionFile, now: &str) -> bool {
    let before = file.dismissed.len();
    file.dismissed.retain(|item| !expired(&item.at, now));
    file.dismissed.len() != before
}

fn expired(at: &str, now: &str) -> bool {
    match (date_ord(at), date_ord(now)) {
        (Some(then), Some(today)) => today - then > DISMISS_DAYS,
        _ => false,
    }
}

fn date_ord(stamp: &str) -> Option<i64> {
    let date = stamp.get(..10)?;
    if !is_date(date) {
        return None;
    }
    let year = date[0..4].parse().ok()?;
    let month = date[5..7].parse().ok()?;
    let day = date[8..10].parse().ok()?;
    Some(days_from_civil(year, month, day))
}

/// Days since 1970-01-01. Valid for the positive years `is_date` accepts.
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let mut y = i64::from(year);
    let m = u64::from(month);
    if m <= 2 {
        y -= 1;
    }
    let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
    let yoe = u64::try_from(y - era * 400).unwrap_or(0);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + u64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + i64::try_from(doe).unwrap_or(0) - 719468
}

fn checked_slug(slug: &str) -> Result<String, SuggestFail> {
    if valid_slug(slug) {
        Ok(slug.to_string())
    } else {
        Err(SuggestFail::new("invalid", "That Bot slug is not valid."))
    }
}

fn checked_text(value: &str) -> Result<String, SuggestFail> {
    let text = strip_source(value.trim());
    check_fact(&text).map_err(|err| SuggestFail::new(err.code, err.message))?;
    Ok(text)
}

fn checked_why(value: &str) -> Result<String, SuggestFail> {
    let why = value.trim();
    if why.is_empty() {
        return Err(SuggestFail::new(
            "invalid",
            "Say why this fact is worth keeping.",
        ));
    }
    if why.chars().count() > WHY_CHARS {
        return Err(SuggestFail::new(
            "invalid",
            format!("Keep the reason to {WHY_CHARS} characters or fewer."),
        ));
    }
    if why.contains('\n') || why.contains('\r') {
        return Err(SuggestFail::new("invalid", "Keep the reason to one line."));
    }
    Ok(why.to_string())
}

fn checked_category(value: Option<&str>) -> Result<Option<String>, SuggestFail> {
    match value.map(str::trim).filter(|label| !label.is_empty()) {
        None => Ok(None),
        Some(raw) => category_label(raw).map(Some).ok_or_else(|| {
            SuggestFail::new(
                "invalid",
                "category must be a short lowercase label (letters, digits, - or _).",
            )
        }),
    }
}

fn lock_key(slug: &str) -> String {
    format!("suggestions:{slug}")
}

fn path(slug: &str) -> String {
    format!(".botapp/memory-suggestions/{slug}.json")
}

async fn read_file(store: &MemoryStore, slug: &str) -> Result<SuggestionFile, SuggestFail> {
    let raw = store.read_text_or_empty(&path(slug)).await?;
    if raw.trim().is_empty() {
        return Ok(SuggestionFile::empty());
    }
    serde_json::from_str(&raw)
        .map_err(|_err| SuggestFail::new("invalid", "The suggestion file is unreadable."))
}

async fn write_file(
    store: &MemoryStore,
    slug: &str,
    file: &SuggestionFile,
) -> Result<(), SuggestFail> {
    let mut text = serde_json::to_string_pretty(file)
        .map_err(|err| SuggestFail::new("invalid", err.to_string()))?;
    text.push('\n');
    store.write_text(&path(slug), &text, Some(0o600)).await?;
    Ok(())
}
