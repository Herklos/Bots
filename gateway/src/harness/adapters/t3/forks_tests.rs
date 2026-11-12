use super::forks::{delete_command, fork_command, merge_command, side_chats_in};
use super::transcript::{latest_run_id, runtime_mode};
use super::turns::runtime_mode_command;
use crate::harness::{ForkPoint, ForkRequest, MergeRequest};
use serde_json::{json, Value};
use std::collections::HashMap;

fn no_environment(command: &Value) {
    assert!(command.get("environmentId").is_none());
}

#[test]
fn fork_merge_and_delete_match_the_nightly_commands() {
    let fork = fork_command(&ForkRequest {
        source_thread_id: "main".into(),
        target_thread_id: "side".into(),
        title: "Side chat".into(),
        side_chat: true,
        point: ForkPoint::LatestStable,
        runtime_mode: None,
    });
    assert_eq!(fork["type"], "thread.fork");
    assert_eq!(fork["createdBy"], "user");
    assert_eq!(fork["creationSource"], "web");
    assert_eq!(fork["sourceThreadId"], "main");
    assert_eq!(fork["targetThreadId"], "side");
    assert_eq!(fork["sourcePoint"], json!({ "type": "latest_stable" }));
    assert_eq!(fork["sideChat"], true);
    assert_eq!(fork["title"], "Side chat");
    assert!(fork.get("commandId").is_none());
    no_environment(&fork);

    let plain = fork_command(&ForkRequest {
        source_thread_id: "main".into(),
        target_thread_id: "other".into(),
        title: "Fork".into(),
        side_chat: false,
        point: ForkPoint::Run("run-9".into()),
        runtime_mode: None,
    });
    assert!(plain.get("sideChat").is_none());
    assert_eq!(
        plain["sourcePoint"],
        json!({ "type": "run", "runId": "run-9" })
    );
    no_environment(&plain);

    let mode = runtime_mode_command("side", "auto");
    assert_eq!(mode["type"], "thread.runtime-mode.set");
    assert_eq!(mode["threadId"], "side");
    assert_eq!(mode["runtimeMode"], "auto");
    no_environment(&mode);

    let merge = merge_command(&MergeRequest {
        source_thread_id: "side".into(),
        target_thread_id: "main".into(),
        run_id: "run-2".into(),
    });
    assert_eq!(merge["type"], "thread.merge_back");
    assert_eq!(merge["createdBy"], "user");
    assert_eq!(merge["creationSource"], "web");
    assert_eq!(merge["sourceThreadId"], "side");
    assert_eq!(merge["targetThreadId"], "main");
    assert_eq!(
        merge["sourcePoint"],
        json!({ "type": "run", "runId": "run-2" })
    );
    no_environment(&merge);

    let delete = delete_command("side");
    assert_eq!(delete["type"], "thread.delete");
    assert_eq!(delete["threadId"], "side");
    assert_eq!(delete["onlyIfUnused"], true);
    no_environment(&delete);
}

#[test]
fn the_filter_keeps_live_side_chats_oldest_first() {
    let parent = json!({
        "contextTransfers": [
            { "type": "fork", "sourceThreadId": "main", "targetThreadId": "later" },
            { "type": "fork", "sourceThreadId": "main", "targetThreadId": "plain" },
            { "type": "fork", "sourceThreadId": "main", "targetThreadId": "gone" },
            { "type": "fork", "sourceThreadId": "other", "targetThreadId": "foreign" },
            { "type": "merge_back", "sourceThreadId": "main", "targetThreadId": "merged" },
            { "type": "fork", "sourceThreadId": "main", "targetThreadId": "missing" },
            { "type": "fork", "sourceThreadId": "main", "targetThreadId": "early" },
        ]
    });
    let mut children = HashMap::new();
    children.insert(
        "later".into(),
        json!({
            "title": "Later",
            "createdAt": "2026-02-01T00:00:00.000Z",
            "lineage": { "sideChat": true },
            "deletedAt": null,
        }),
    );
    children.insert(
        "plain".into(),
        json!({
            "title": "Not a side chat",
            "createdAt": "2026-01-01T00:00:00.000Z",
            "lineage": { "sideChat": false },
        }),
    );
    children.insert(
        "gone".into(),
        json!({
            "title": "Deleted",
            "createdAt": "2026-01-02T00:00:00.000Z",
            "lineage": { "sideChat": true },
            "deletedAt": "2026-03-01T00:00:00.000Z",
        }),
    );
    children.insert(
        "foreign".into(),
        json!({
            "title": "Other parent",
            "createdAt": "2026-01-01T00:00:00.000Z",
            "lineage": { "sideChat": true },
        }),
    );
    children.insert(
        "merged".into(),
        json!({
            "title": "Merged",
            "createdAt": "2026-01-01T00:00:00.000Z",
            "lineage": { "sideChat": true },
        }),
    );
    children.insert(
        "early".into(),
        json!({
            "thread": {
                "createdAt": "2026-01-01T00:00:00.000Z",
                "lineage": { "sideChat": true },
            }
        }),
    );
    let chats = side_chats_in("main", &parent, &children);
    assert_eq!(chats.len(), 2);
    assert_eq!(chats[0].id, "early");
    assert_eq!(chats[0].title, "Side chat");
    assert_eq!(chats[1].id, "later");
    assert_eq!(chats[1].title, "Later");
}

#[test]
fn runtime_mode_and_the_latest_run_read_the_projection() {
    assert_eq!(
        runtime_mode(&json!({ "runtimeMode": "auto" })).as_deref(),
        Some("auto")
    );
    assert_eq!(
        runtime_mode(&json!({ "thread": { "runtimeMode": "full-access" } })).as_deref(),
        Some("full-access")
    );
    assert!(runtime_mode(&json!({ "runtimeMode": "ask" })).is_none());
    assert_eq!(
        latest_run_id(&json!({ "runs": [{ "id": "r1" }, { "id": "r2" }] })).as_deref(),
        Some("r2")
    );
    assert!(latest_run_id(&json!({ "runs": [] })).is_none());
    assert_eq!(
        latest_run_id(&json!({ "latestRunId": "r9" })).as_deref(),
        Some("r9")
    );
}
