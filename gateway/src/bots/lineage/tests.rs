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
