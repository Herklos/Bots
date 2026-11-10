//! A Bot's thread family: subagents and delegated child threads.
//! A remote child is never replaced with the parent thread.
use serde_json::{json, Value};

/// T3 follows a real shell snapshot with a project-only one. That empty thread list is not the roster.
pub fn is_project_enrichment(item: &Value) -> bool {
    item.get("resolvedRepositoryIdentityRoots").is_some()
        && item["snapshot"]["threads"]
            .as_array()
            .is_some_and(|t| t.is_empty())
        && item["snapshot"]["archivedThreads"]
            .as_array()
            .map(|t| t.is_empty())
            .unwrap_or(true)
}

pub fn status_label(raw: &str, pending_request: bool, settled: bool) -> &'static str {
    if pending_request {
        return "Waiting";
    }
    match raw {
        "pending" | "preparing" | "queued" | "starting" | "running" => "Working",
        "waiting" => "Waiting",
        "failed" => "Failed",
        "cancelled" => "Cancelled",
        "interrupted" => "Stopped",
        "rolled_back" => "Rolled back",
        "completed" | "settled" => "Done",
        "idle" => "Idle",
        _ if settled => "Done",
        _ => "Unknown",
    }
}

/// Same live statuses the transcript uses for a turn that is still going.
const LIVE: &[&str] = &[
    "running",
    "in_progress",
    "pending",
    "queued",
    "streaming",
    "starting",
];

/// Keys that mark one child: its item id, and its thread when T3 gives one.
/// A turn item and the `subagents` roster often use different ids for the same child.
fn row_keys(row: &Value) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(id) = row["id"].as_str().filter(|s| !s.is_empty()) {
        keys.push(id.to_string());
    }
    if let Some(thread) = row["threadId"].as_str().filter(|s| !s.is_empty()) {
        keys.push(format!("thread:{thread}"));
    }
    keys
}

fn push_unique(out: &mut Vec<Value>, seen: &mut std::collections::HashSet<String>, row: Value) {
    let keys = row_keys(&row);
    if keys.is_empty() || keys.iter().any(|k| seen.contains(k)) {
        return;
    }
    for key in keys {
        seen.insert(key);
    }
    out.push(row);
}

/// Working on keeps rows that still need attention.
fn is_active(row: &Value) -> bool {
    matches!(
        row["status"].as_str(),
        Some("Working" | "Waiting" | "Failed" | "Stopped")
    )
}

/// Rows for the Bot details sheet. Remote delegates stay remote.
/// Settled children (Done, Idle, …) stay out. With no active child, the latest
/// live turn is one Working row and not an Open target.
pub fn rows(projection: &Value) -> Vec<Value> {
    if is_project_enrichment(projection) {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let items = projection["turnItems"]
        .as_array()
        .or_else(|| projection["visibleTurnItems"].as_array());
    if let Some(items) = items {
        for raw in items {
            let item = if raw["item"].is_object() {
                &raw["item"]
            } else {
                raw
            };
            if item["type"] == "subagent" {
                if let Some(row) = one(item) {
                    push_unique(&mut out, &mut seen, row);
                }
            }
        }
    }
    if let Some(agents) = projection["subagents"].as_array() {
        for agent in agents {
            if let Some(row) = one(agent) {
                push_unique(&mut out, &mut seen, row);
            }
        }
    }
    out.retain(is_active);
    if out.is_empty() {
        if let Some(items) = items {
            if let Some(row) = live_turn(items) {
                out.push(row);
            }
        }
    }
    out
}

fn item_of(raw: &Value) -> &Value {
    if raw["item"].is_object() {
        &raw["item"]
    } else {
        raw
    }
}

fn tool_segment(name: &str) -> &str {
    name.rsplit("__").next().unwrap_or(name)
}

/// The latest live item that is not a user message, as one row.
fn live_turn(items: &[Value]) -> Option<Value> {
    let item = items.iter().rev().map(item_of).find(|item| {
        let kind = item["type"].as_str().unwrap_or("");
        let status = item["status"].as_str().unwrap_or("");
        let live = LIVE.contains(&status) || item["streaming"] == true;
        live && kind != "user_message" && item["id"].as_str().is_some_and(|id| !id.is_empty())
    })?;
    let tool = item["toolName"]
        .as_str()
        .map(tool_segment)
        .filter(|s| !s.is_empty());
    let title = tool
        .or_else(|| item["label"].as_str().filter(|s| !s.is_empty()))
        .or_else(|| item["title"].as_str().filter(|s| !s.is_empty()))
        .unwrap_or("Working");
    Some(json!({
        "id": item["id"].as_str().unwrap_or(""),
        "title": title,
        "status": "Working",
        "remote": false,
        "threadId": "",
    }))
}

fn one(agent: &Value) -> Option<Value> {
    let id = agent["id"].as_str().filter(|s| !s.is_empty())?;
    let remote_thread = agent["remoteChild"]["threadId"]
        .as_str()
        .filter(|s| !s.is_empty());
    let remote_env = agent["remoteChild"]["environmentId"]
        .as_str()
        .filter(|s| !s.is_empty());
    let local = agent["childThreadId"].as_str().filter(|s| !s.is_empty());
    let (thread_id, remote) = match (remote_thread, remote_env, local) {
        (Some(thread), Some(_), _) => (thread, true),
        (_, _, Some(thread)) => (thread, false),
        _ => ("", false),
    };
    let raw = agent["activityRunStatus"]
        .as_str()
        .or_else(|| agent["status"].as_str())
        .unwrap_or("");
    let pending = !agent["pendingRuntimeRequest"].is_null()
        && agent.get("pendingRuntimeRequest").is_some()
        && agent["pendingRuntimeRequest"] != json!(null)
        && agent["pendingRuntimeRequest"] != false;
    let settled = agent.get("settledAt").and_then(|v| v.as_str()).is_some() || raw == "settled";
    let title = agent["title"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| agent["label"].as_str().filter(|s| !s.is_empty()))
        .unwrap_or("Child");
    Some(json!({
        "id": id,
        "title": title,
        "status": status_label(raw, pending, settled),
        "remote": remote,
        "threadId": thread_id,
    }))
}

/// A local child opens as that thread. A remote child does not open, and never as the parent.
pub fn open_child(row: &Value, parent: &str) -> Option<String> {
    if row["remote"] == true {
        return None;
    }
    let thread = row["threadId"].as_str().unwrap_or("");
    if thread.is_empty() || thread == parent {
        None
    } else {
        Some(thread.to_string())
    }
}

/// The thread a send, stop, voice clip, or request answer uses.
/// No child means the parent. A local child is that thread. A remote child, or the parent id
/// passed as the child, is refused. This writes nothing.
pub fn thread_for(
    rows: &[Value],
    parent: &str,
    asked: Option<&str>,
) -> Result<String, &'static str> {
    let Some(child) = asked.map(str::trim).filter(|id| !id.is_empty()) else {
        return Ok(parent.to_string());
    };
    rows.iter()
        .find(|row| row["threadId"].as_str() == Some(child))
        .and_then(|row| open_child(row, parent))
        .ok_or("That thread stays on the other machine.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_follow_the_thread_family_rules() {
        assert_eq!(status_label("running", false, false), "Working");
        assert_eq!(status_label("running", true, false), "Waiting");
        assert_eq!(status_label("failed", false, false), "Failed");
        assert_eq!(status_label("cancelled", false, false), "Cancelled");
        assert_eq!(status_label("interrupted", false, false), "Stopped");
        assert_eq!(status_label("rolled_back", false, false), "Rolled back");
        assert_eq!(status_label("idle", false, false), "Idle");
        assert_eq!(status_label("mystery", false, true), "Done");
        assert_eq!(status_label("mystery", false, false), "Unknown");
    }

    #[test]
    fn remote_child_is_not_replaced_by_the_parent() {
        let projection = json!({
            "turnItems": [{
                "type": "subagent",
                "id": "a1",
                "title": "Lookup",
                "status": "running",
                "remoteChild": { "threadId": "remote-1", "environmentId": "env-9", "label": "Other" },
                "childThreadId": "local-should-not-win"
            }],
            "subagents": [{ "id": "a2", "title": "Notes", "status": "failed", "childThreadId": "local-2" }]
        });
        let rows = rows(&projection);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["remote"], true);
        assert_eq!(rows[0]["threadId"], "remote-1");
        assert_eq!(rows[0]["status"], "Working");
        assert_eq!(rows[1]["remote"], false);
        assert_eq!(rows[1]["threadId"], "local-2");
        assert_eq!(rows[1]["status"], "Failed");
    }

    #[test]
    fn a_project_enrichment_snapshot_adds_no_rows() {
        let item = json!({
            "resolvedRepositoryIdentityRoots": ["r"],
            "snapshot": { "threads": [], "archivedThreads": [] },
            "turnItems": [{ "id": "live", "type": "tool_call", "status": "running", "toolName": "read_file" }]
        });
        assert!(is_project_enrichment(&item));
        assert!(rows(&item).is_empty());
    }

    #[test]
    fn an_agent_without_a_child_thread_stays_on_this_bot() {
        let rows = rows(&json!({
            "subagents": [{ "id": "a", "title": "Think", "status": "waiting" }]
        }));
        assert_eq!(rows[0]["threadId"], "");
        assert_eq!(rows[0]["remote"], false);
        assert_eq!(rows[0]["status"], "Waiting");
    }

    #[test]
    fn a_nested_item_is_read_once_and_a_pending_request_waits() {
        let listed = rows(&json!({
            "visibleTurnItems": [{
                "item": {
                    "type": "subagent",
                    "id": "same",
                    "title": "Lookup",
                    "status": "running",
                    "activityRunStatus": "failed",
                    "pendingRuntimeRequest": { "kind": "ask" },
                    "remoteChild": { "threadId": "remote-only" },
                    "childThreadId": "local-1"
                }
            }],
            "subagents": [{ "id": "same", "title": "Again", "status": "completed", "childThreadId": "other" }]
        }));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["status"], "Waiting");
        assert_eq!(listed[0]["remote"], false);
        assert_eq!(listed[0]["threadId"], "local-1");
    }

    #[test]
    fn a_full_remote_child_wins_and_a_false_request_does_not_wait() {
        let listed = rows(&json!({
            "subagents": [{
                "id": "r",
                "label": "Away",
                "status": "running",
                "pendingRuntimeRequest": false,
                "remoteChild": { "threadId": "remote-2", "environmentId": "env" },
                "childThreadId": "local-2"
            }]
        }));
        assert_eq!(listed[0]["remote"], true);
        assert_eq!(listed[0]["threadId"], "remote-2");
        assert_eq!(listed[0]["title"], "Away");
        assert_eq!(listed[0]["status"], "Working");
    }

    #[test]
    fn settled_work_is_labelled_done_and_stays_out_of_working_on() {
        assert_eq!(status_label("mystery", false, true), "Done");
        assert!(rows(&json!({
            "turnItems": [{
                "type": "subagent",
                "id": "s",
                "title": "Wrap",
                "status": "mystery",
                "settledAt": "2026-11-10T00:00:00Z"
            }]
        }))
        .is_empty());
        assert!(rows(&json!({ "turnItems": [{ "type": "message", "id": "m" }] })).is_empty());
    }

    #[test]
    fn a_completed_child_stays_out_and_a_failed_child_stays() {
        assert!(rows(&json!({
            "subagents": [{
                "id": "done",
                "title": "/root/test",
                "status": "completed",
                "childThreadId": "thread-done"
            }]
        }))
        .is_empty());
        let failed = rows(&json!({
            "subagents": [{
                "id": "bad",
                "title": "Broke",
                "status": "failed",
                "childThreadId": "thread-bad"
            }]
        }));
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0]["status"], "Failed");
        assert_eq!(failed[0]["threadId"], "thread-bad");
    }

    #[test]
    fn the_same_child_in_turn_items_and_subagents_is_one_row() {
        let listed = rows(&json!({
            "turnItems": [{
                "type": "subagent",
                "id": "call_abc:child-1",
                "title": "/root/test",
                "status": "running",
                "childThreadId": "thread:provider:codex:native-thread:child-1"
            }],
            "subagents": [{
                "id": "node:provider:codex:native-node:call_abc:child-1",
                "title": "/root/test",
                "status": "running",
                "childThreadId": "thread:provider:codex:native-thread:child-1"
            }]
        }));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["title"], "/root/test");
        assert_eq!(
            listed[0]["threadId"],
            "thread:provider:codex:native-thread:child-1"
        );
        assert_eq!(listed[0]["status"], "Working");
    }

    #[test]
    fn a_running_tool_with_no_subagent_is_one_working_row() {
        let listed = rows(&json!({
            "turnItems": [
                { "id": "old", "type": "tool_call", "status": "completed", "toolName": "mcp__x__gone" },
                { "id": "user", "type": "user_message", "status": "running", "text": "hi" },
                { "id": "live", "type": "tool_call", "status": "running", "toolName": "mcp__t3-code__read_file" }
            ]
        }));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["id"], "live");
        assert_eq!(listed[0]["title"], "read_file");
        assert_eq!(listed[0]["status"], "Working");
        assert_eq!(listed[0]["remote"], false);
        assert_eq!(listed[0]["threadId"], "");
        assert_eq!(open_child(&listed[0], "parent"), None);
    }

    #[test]
    fn an_idle_projection_with_no_subagent_adds_no_row() {
        assert!(rows(&json!({
            "turnItems": [
                { "id": "done", "type": "tool_call", "status": "completed", "toolName": "read_file" },
                { "id": "note", "type": "assistant_message", "status": "idle", "text": "ok" }
            ]
        }))
        .is_empty());
    }

    #[test]
    fn a_local_child_opens_and_a_remote_child_does_not_become_the_parent() {
        let projection = json!({
            "turnItems": [{
                "type": "subagent",
                "id": "away",
                "title": "Away",
                "status": "running",
                "remoteChild": { "threadId": "remote-1", "environmentId": "env-9" },
                "childThreadId": "should-not-win"
            }],
            "subagents": [{
                "id": "notes",
                "title": "Notes",
                "status": "failed",
                "childThreadId": "child-thread"
            }]
        });
        let listed = rows(&projection);
        let remote = listed.iter().find(|row| row["remote"] == true).unwrap();
        let local = listed.iter().find(|row| row["remote"] == false).unwrap();
        assert_eq!(
            open_child(local, "parent-thread").as_deref(),
            Some("child-thread")
        );
        assert_eq!(open_child(remote, "parent-thread"), None);
        assert_ne!(
            open_child(remote, "parent-thread").as_deref(),
            Some("parent-thread")
        );
        assert_eq!(
            open_child(
                &json!({ "remote": false, "threadId": "parent-thread" }),
                "parent-thread"
            ),
            None
        );
    }

    fn family() -> Value {
        json!({
            "turnItems": [{
                "type": "subagent",
                "id": "away",
                "title": "Away",
                "status": "running",
                "remoteChild": { "threadId": "remote-1", "environmentId": "env-9" },
                "childThreadId": "should-not-win"
            }],
            "subagents": [{
                "id": "notes",
                "title": "Notes",
                "status": "failed",
                "childThreadId": "child-thread"
            }]
        })
    }

    #[test]
    fn resolving_a_local_child_returns_that_thread_and_refuses_a_remote_or_the_parent() {
        let mut rows = rows(&family());
        rows.push(json!({ "remote": false, "threadId": "parent-thread", "id": "self" }));
        assert_eq!(
            thread_for(&rows, "parent-thread", Some("child-thread")).as_deref(),
            Ok("child-thread")
        );
        assert_eq!(
            thread_for(&rows, "parent-thread", None).as_deref(),
            Ok("parent-thread")
        );
        assert_eq!(
            thread_for(&rows, "parent-thread", Some("  ")).as_deref(),
            Ok("parent-thread")
        );
        assert!(thread_for(&rows, "parent-thread", Some("remote-1")).is_err());
        assert!(thread_for(&rows, "parent-thread", Some("parent-thread")).is_err());
        assert_ne!(
            thread_for(&rows, "parent-thread", Some("remote-1")).as_deref(),
            Ok("parent-thread")
        );
        assert_ne!(
            thread_for(&rows, "parent-thread", Some("parent-thread")).as_deref(),
            Ok("parent-thread")
        );
    }
}
