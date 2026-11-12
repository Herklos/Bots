//! Where a history search reads.
//!
//! [`T3Search`] asks the harness for a page of matches, drops every thread
//! outside the allowlist before reading it, and keeps a survivor only when
//! that message is still in `transcript()`. [`LocalScan`] scores visible
//! messages newest first. It runs when the command is missing, the call
//! fails, or a full page leaves fewer than three allowlisted hits.

use super::cache::ProjectionCache;
use super::score::{score, Query, Scored};
use super::window::{routine_ids, Role};
use crate::harness::t3::transcript::transcript;
use crate::harness::{HarnessError, ThreadHit, ThreadSearch, Transcript};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};

const LIMIT: u32 = 50;
const CROWD_MIN: usize = 3;
const MATCH_CAP: usize = 10;
const MAX_MESSAGES: usize = 5_000;
const MAX_BYTES: usize = 2 * 1024 * 1024;
const SNIPPET_CHARS: usize = 240;

/// One ranked visible message.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Match {
    pub thread_id: String,
    pub message_id: String,
    pub role: Role,
    pub at: String,
    pub snippet: String,
    pub hits: u64,
    pub quality: f64,
}

/// Ranked matches, and whether older history was left unread.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Found {
    pub matches: Vec<Match>,
    pub truncated: bool,
}

pub(crate) enum Step {
    Fallback,
    Stop(HarnessError),
}

#[async_trait]
pub(crate) trait Searcher {
    async fn search(&self, query: &Query, allowed: &BTreeSet<String>) -> Result<Found, Step>;
}

pub(crate) async fn find(
    threads: &dyn ThreadSearch,
    reader: &dyn Transcript,
    cache: &ProjectionCache,
    query_text: &str,
    allowed: &BTreeSet<String>,
    stamps: &HashMap<String, String>,
) -> Result<Found, HarnessError> {
    if allowed.is_empty() {
        return Ok(empty());
    }
    let Some(query) = Query::prepare(query_text) else {
        return Ok(empty());
    };
    let t3 = T3Search {
        threads,
        reader,
        cache,
        stamps,
        query_text,
    };
    match t3.search(&query, allowed).await {
        Ok(found) => return Ok(found),
        Err(Step::Fallback) => {}
        Err(Step::Stop(err)) => return Err(err),
    }
    let local = LocalScan {
        reader,
        cache,
        stamps,
    };
    match local.search(&query, allowed).await {
        Ok(found) => Ok(found),
        Err(Step::Fallback) => Err(HarnessError::failed("history search failed")),
        Err(Step::Stop(err)) => Err(err),
    }
}

fn empty() -> Found {
    Found {
        matches: Vec::new(),
        truncated: false,
    }
}

struct T3Search<'a> {
    threads: &'a dyn ThreadSearch,
    reader: &'a dyn Transcript,
    cache: &'a ProjectionCache,
    stamps: &'a HashMap<String, String>,
    query_text: &'a str,
}

struct LocalScan<'a> {
    reader: &'a dyn Transcript,
    cache: &'a ProjectionCache,
    stamps: &'a HashMap<String, String>,
}

#[async_trait]
impl Searcher for T3Search<'_> {
    async fn search(&self, query: &Query, allowed: &BTreeSet<String>) -> Result<Found, Step> {
        let text = self.query_text.trim();
        if !(2..=200).contains(&text.chars().count()) {
            return Err(Step::Fallback);
        }
        let raw = self
            .threads
            .search_threads(text, LIMIT)
            .await
            .map_err(step_from)?;
        let total = raw.len();
        let own: Vec<ThreadHit> = raw
            .into_iter()
            .filter(|hit| allowed.contains(&hit.thread_id))
            .collect();
        let full_page = u32::try_from(total).unwrap_or(u32::MAX) >= LIMIT;
        if full_page && own.len() < CROWD_MIN {
            return Err(Step::Fallback);
        }
        let mut pages = Pages::new(self.reader, self.cache, self.stamps);
        let mut by_thread: HashMap<String, Vec<Line>> = HashMap::new();
        let mut partial = false;
        for hit in &own {
            if by_thread.contains_key(&hit.thread_id) {
                continue;
            }
            let body = pages.load(&hit.thread_id).await?;
            partial |= is_partial(&body);
            by_thread.insert(hit.thread_id.clone(), lines_of(&hit.thread_id, &body));
        }
        let mut kept = Vec::new();
        for hit in &own {
            let Some(lines) = by_thread.get(&hit.thread_id) else {
                continue;
            };
            let Some(line) = locate(lines, hit) else {
                continue;
            };
            let Some(scored) = score(query, &line.text) else {
                continue;
            };
            kept.push(Kept {
                line: line.clone(),
                scored,
                reported_hits: hit.hits,
            });
        }
        Ok(finish(kept, partial, false))
    }
}

#[async_trait]
impl Searcher for LocalScan<'_> {
    async fn search(&self, query: &Query, allowed: &BTreeSet<String>) -> Result<Found, Step> {
        let mut pages = Pages::new(self.reader, self.cache, self.stamps);
        let mut lines = Vec::new();
        let mut partial = false;
        for thread in allowed {
            let body = pages.load(thread).await?;
            partial |= is_partial(&body);
            lines.extend(lines_of(thread, &body));
        }
        lines.sort_by(|left, right| {
            right
                .at
                .cmp(&left.at)
                .then(right.index.cmp(&left.index))
                .then(left.thread_id.cmp(&right.thread_id))
                .then(left.id.cmp(&right.id))
        });
        let mut kept = Vec::new();
        let mut bytes = 0usize;
        let mut stopped = false;
        for (count, line) in lines.into_iter().enumerate() {
            if count >= MAX_MESSAGES || bytes >= MAX_BYTES {
                stopped = true;
                break;
            }
            bytes = bytes.saturating_add(line.text.len());
            if let Some(scored) = score(query, &line.text) {
                kept.push(Kept {
                    line,
                    scored,
                    reported_hits: None,
                });
            }
        }
        Ok(finish(kept, partial, stopped))
    }
}

struct Pages<'a> {
    reader: &'a dyn Transcript,
    cache: &'a ProjectionCache,
    stamps: &'a HashMap<String, String>,
    seen: HashMap<String, Value>,
}

impl<'a> Pages<'a> {
    fn new(
        reader: &'a dyn Transcript,
        cache: &'a ProjectionCache,
        stamps: &'a HashMap<String, String>,
    ) -> Self {
        Self {
            reader,
            cache,
            stamps,
            seen: HashMap::new(),
        }
    }

    async fn load(&mut self, thread: &str) -> Result<Value, Step> {
        if let Some(body) = self.seen.get(thread) {
            return Ok(body.clone());
        }
        let stamp = self.stamps.get(thread).map(String::as_str).unwrap_or("");
        let body = match self.cache.get(thread, stamp) {
            Some(body) => body,
            None => {
                let body = self
                    .reader
                    .read(thread)
                    .await
                    .map_err(Step::Stop)?
                    .body()
                    .clone();
                self.cache.store(thread, stamp, &body);
                body
            }
        };
        self.seen.insert(thread.to_string(), body.clone());
        Ok(body)
    }
}

fn step_from(err: HarnessError) -> Step {
    match err {
        HarnessError::Unsupported(_) | HarnessError::Failed(_) | HarnessError::Unavailable(_) => {
            Step::Fallback
        }
    }
}

#[derive(Clone)]
struct Line {
    thread_id: String,
    id: String,
    role: Role,
    at: String,
    text: String,
    index: usize,
}

struct Kept {
    line: Line,
    scored: Scored,
    reported_hits: Option<u64>,
}

fn lines_of(thread_id: &str, projection: &Value) -> Vec<Line> {
    let routines = routine_ids(projection);
    let view = transcript(projection);
    view["messages"]
        .as_array()
        .map(|messages| {
            messages
                .iter()
                .enumerate()
                .filter_map(|(index, message)| one_line(thread_id, index, message, &routines))
                .collect()
        })
        .unwrap_or_default()
}

fn one_line(
    thread_id: &str,
    index: usize,
    message: &Value,
    routines: &HashSet<String>,
) -> Option<Line> {
    let id = message["id"].as_str()?.to_string();
    let name = message["role"].as_str().unwrap_or("");
    let role = if routines.contains(&id) {
        Role::Routine
    } else if name == "user" {
        Role::You
    } else {
        Role::Bot
    };
    Some(Line {
        thread_id: thread_id.to_string(),
        id,
        role,
        at: message["at"].as_str().unwrap_or("").to_string(),
        text: message["text"].as_str().unwrap_or("").to_string(),
        index,
    })
}

fn is_partial(projection: &Value) -> bool {
    projection["truncated"] == true
        || projection["hasMore"] == true
        || projection["partial"] == true
}

fn locate<'a>(lines: &'a [Line], hit: &ThreadHit) -> Option<&'a Line> {
    if let Some(id) = &hit.message_id {
        return lines.iter().find(|line| &line.id == id);
    }
    let needle = snippet_needle(&hit.snippet).to_lowercase();
    if needle.is_empty() {
        return None;
    }
    let matched: Vec<&Line> = lines
        .iter()
        .filter(|line| {
            source_ok(&hit.source, line.role) && line.text.to_lowercase().contains(&needle)
        })
        .collect();
    if let Some(at) = &hit.message_created_at {
        if let Some(line) = matched.iter().copied().rev().find(|line| &line.at == at) {
            return Some(line);
        }
    }
    matched.last().copied()
}

fn source_ok(source: &str, role: Role) -> bool {
    matches!(
        (source, role),
        ("user", Role::You | Role::Routine) | ("assistant", Role::Bot)
    )
}

fn snippet_needle(snippet: &str) -> String {
    let trimmed = snippet.trim();
    let trimmed = trimmed.strip_prefix('…').unwrap_or(trimmed).trim();
    let trimmed = trimmed.strip_prefix("...").unwrap_or(trimmed).trim();
    let trimmed = trimmed.strip_suffix('…').unwrap_or(trimmed).trim();
    trimmed
        .strip_suffix("...")
        .unwrap_or(trimmed)
        .trim()
        .to_string()
}

fn finish(kept: Vec<Kept>, partial: bool, stopped: bool) -> Found {
    let kept = dedup(kept);
    let mut counted: HashMap<String, u64> = HashMap::new();
    let mut reported: HashMap<String, u64> = HashMap::new();
    for item in &kept {
        let count = counted.entry(item.line.thread_id.clone()).or_insert(0);
        *count = count.saturating_add(1);
        if let Some(hits) = item.reported_hits {
            let slot = reported.entry(item.line.thread_id.clone()).or_insert(hits);
            *slot = (*slot).max(hits);
        }
    }
    let mut kept = kept;
    kept.sort_by(|left, right| {
        right
            .scored
            .quality
            .total_cmp(&left.scored.quality)
            .then_with(|| right.line.at.cmp(&left.line.at))
            .then_with(|| right.line.index.cmp(&left.line.index))
            .then_with(|| left.line.thread_id.cmp(&right.line.thread_id))
            .then_with(|| left.line.id.cmp(&right.line.id))
    });
    kept.truncate(MATCH_CAP);
    let matches = kept
        .into_iter()
        .map(|item| {
            let counted = counted.get(&item.line.thread_id).copied().unwrap_or(1);
            let hits = reported
                .get(&item.line.thread_id)
                .copied()
                .map(|hits| hits.max(counted))
                .unwrap_or(counted);
            Match {
                thread_id: item.line.thread_id,
                message_id: item.line.id,
                role: item.line.role,
                at: item.line.at,
                snippet: snippet(&item.line.text, item.scored.offset),
                hits,
                quality: item.scored.quality,
            }
        })
        .collect();
    Found {
        matches,
        truncated: partial || stopped,
    }
}

fn dedup(kept: Vec<Kept>) -> Vec<Kept> {
    let mut best: HashMap<(String, String), usize> = HashMap::new();
    let mut out: Vec<Kept> = Vec::new();
    for item in kept {
        let key = (item.line.thread_id.clone(), item.line.id.clone());
        if let Some(&index) = best.get(&key) {
            let hits = max_opt(out[index].reported_hits, item.reported_hits);
            if item
                .scored
                .quality
                .total_cmp(&out[index].scored.quality)
                .is_gt()
            {
                out[index] = item;
            }
            out[index].reported_hits = hits;
        } else {
            best.insert(key, out.len());
            out.push(item);
        }
    }
    out
}

fn max_opt(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn snippet(text: &str, offset: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= SNIPPET_CHARS {
        return text.to_string();
    }
    let at = offset.min(chars.len().saturating_sub(1));
    let start = at
        .saturating_sub(80)
        .min(chars.len().saturating_sub(SNIPPET_CHARS));
    let end = (start + SNIPPET_CHARS).min(chars.len());
    chars[start..end].iter().collect()
}
