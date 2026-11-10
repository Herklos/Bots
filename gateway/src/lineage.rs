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

/// Rows for the Bot details sheet. Remote delegates stay remote.
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
                    seen.insert(row["id"].as_str().unwrap_or("").to_string());
                    out.push(row);
                }
            }
        }
    }
    if let Some(agents) = projection["subagents"].as_array() {
        for agent in agents {
            if let Some(row) = one(agent) {
                let id = row["id"].as_str().unwrap_or("").to_string();
                if seen.insert(id) {
                    out.push(row);
                }
            }
        }
    }
    out
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
            "subagents": [{ "id": "a2", "title": "Notes", "status": "completed", "childThreadId": "local-2" }]
        });
        let rows = rows(&projection);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["remote"], true);
        assert_eq!(rows[0]["threadId"], "remote-1");
        assert_eq!(rows[0]["status"], "Working");
        assert_eq!(rows[1]["remote"], false);
        assert_eq!(rows[1]["threadId"], "local-2");
        assert_eq!(rows[1]["status"], "Done");
    }

    #[test]
    fn a_project_enrichment_snapshot_adds_no_rows() {
        let item = json!({
            "resolvedRepositoryIdentityRoots": ["r"],
            "snapshot": { "threads": [], "archivedThreads": [] }
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
    fn settled_work_is_done_even_when_the_word_is_unknown() {
        let listed = rows(&json!({
            "turnItems": [{
                "type": "subagent",
                "id": "s",
                "title": "Wrap",
                "status": "mystery",
                "settledAt": "2026-11-10T00:00:00Z"
            }]
        }));
        assert_eq!(listed[0]["status"], "Done");
        assert!(rows(&json!({ "turnItems": [{ "type": "message", "id": "m" }] })).is_empty());
    }
}
