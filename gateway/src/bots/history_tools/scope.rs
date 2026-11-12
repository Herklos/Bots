//! Threads one caller may search.
//!
//! A Bot may search its own main thread and the local child threads in its
//! lineage rows, including a child that has already finished. A subagent
//! searches only its own thread. A remote child stays on the other machine.
//! Group room threads are excluded. A hit from any other thread is dropped
//! before anything else is read.

use crate::bots::{is_subagent, lineage};
use serde_json::Value;
use std::collections::{BTreeSet, HashSet};

/// One ranked hit, before it is checked against the allowlist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub thread_id: String,
    pub snippet: String,
}

/// The caller's own thread, plus local child threads, minus group rooms.
///
/// `rows` use the lineage shape (`threadId`, `remote`). A subagent ignores
/// `rows`: parent history is out of reach.
pub(crate) fn allowlist(
    caller: &Value,
    rows: &[Value],
    group_threads: &HashSet<String>,
) -> BTreeSet<String> {
    let own = caller["mainThreadId"].as_str().unwrap_or("").trim();
    if own.is_empty() {
        return BTreeSet::new();
    }
    if is_subagent(caller) {
        return one(own, group_threads);
    }
    let mut allowed = one(own, group_threads);
    for row in rows {
        if let Some(thread) = lineage::open_child(row, own) {
            insert(&mut allowed, &thread, group_threads);
        }
    }
    allowed
}

/// Drops every hit whose thread is outside `allowed`, and keeps the rest in order.
pub(crate) fn retain_allowed(hits: Vec<Candidate>, allowed: &BTreeSet<String>) -> Vec<Candidate> {
    hits.into_iter()
        .filter(|hit| allowed.contains(&hit.thread_id))
        .collect()
}

fn one(thread: &str, group_threads: &HashSet<String>) -> BTreeSet<String> {
    let mut allowed = BTreeSet::new();
    insert(&mut allowed, thread, group_threads);
    allowed
}

fn insert(allowed: &mut BTreeSet<String>, thread: &str, group_threads: &HashSet<String>) {
    if thread.is_empty() || group_threads.contains(thread) {
        return;
    }
    allowed.insert(thread.to_string());
}
