use super::scope::{allowlist, retain_allowed, Candidate};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashSet};

fn caller(thread: &str, role: &str, hidden: bool) -> Value {
    json!({
        "slug": "ada",
        "mainThreadId": thread,
        "role": role,
        "hideFromSidebar": hidden,
    })
}

fn child(thread: &str, remote: bool) -> Value {
    json!({
        "id": thread,
        "threadId": thread,
        "remote": remote,
        "title": "Child",
    })
}

fn hit(thread: &str, snippet: &str) -> Candidate {
    Candidate {
        thread_id: thread.to_string(),
        snippet: snippet.to_string(),
    }
}

fn threads(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).to_string()).collect()
}

#[test]
fn a_subagent_searches_only_its_own_thread() {
    let rows = vec![
        child("thread-parent", false),
        child("thread-sibling", false),
        child("thread-own-child", false),
    ];
    let subagent = caller("thread-child", "subagent", true);
    let allowed = allowlist(&subagent, &rows, &HashSet::new());
    assert_eq!(allowed, threads(&["thread-child"]));

    let hidden = caller("thread-hidden", "bot", true);
    assert_eq!(
        allowlist(&hidden, &rows, &HashSet::new()),
        threads(&["thread-hidden"])
    );

    let kept = retain_allowed(
        vec![
            hit("thread-parent", "parent-only-secret"),
            hit("thread-child", "my note"),
        ],
        &allowed,
    );
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].snippet, "my note");
    assert!(kept
        .iter()
        .all(|hit| !hit.snippet.contains("parent-only-secret")));
}

#[test]
fn group_room_threads_are_left_out() {
    let rows = vec![child("thread-child", false), child("thread-group", false)];
    let mut groups = HashSet::new();
    groups.insert("thread-group".to_string());
    groups.insert("thread-room".to_string());
    let allowed = allowlist(&caller("thread-ada", "bot", false), &rows, &groups);
    assert_eq!(allowed, threads(&["thread-ada", "thread-child"]));

    let kept = retain_allowed(
        vec![
            hit("thread-group", "group-only-secret"),
            hit("thread-ada", "ada said hello"),
        ],
        &allowed,
    );
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].snippet, "ada said hello");
    assert!(kept
        .iter()
        .all(|hit| !hit.snippet.contains("group-only-secret")));

    let only_group = allowlist(&caller("thread-group", "bot", false), &[], &groups);
    assert!(only_group.is_empty());
}

#[test]
fn another_bots_snippet_is_dropped_even_when_ranked_first() {
    let rows = vec![child("thread-child", false)];
    let allowed = allowlist(&caller("thread-ada", "bot", false), &rows, &HashSet::new());
    let kept = retain_allowed(
        vec![
            hit("thread-bea", "bea-only-secret"),
            hit("thread-ada", "ada said hello"),
            hit("thread-bea", "bea-only-secret again"),
            hit("thread-child", "child note"),
        ],
        &allowed,
    );
    assert_eq!(
        kept.iter()
            .map(|hit| hit.snippet.as_str())
            .collect::<Vec<_>>(),
        vec!["ada said hello", "child note"]
    );
    assert!(kept
        .iter()
        .all(|hit| !hit.snippet.contains("bea-only-secret")));
}

#[test]
fn a_bot_keeps_local_children_and_not_a_remote_one() {
    let rows = vec![
        child("thread-child", false),
        child("thread-remote", true),
        child("thread-ada", false),
    ];
    let allowed = allowlist(&caller("thread-ada", "bot", false), &rows, &HashSet::new());
    assert_eq!(allowed, threads(&["thread-ada", "thread-child"]));
    assert!(!allowed.contains("thread-remote"));
}
