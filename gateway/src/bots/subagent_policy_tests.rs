use super::*;
use crate::bots::helpers_policy::{at_limit_message, HELPERS_OFF};
use crate::bots::Bots;
use serde_json::{json, Value};
use tokio::sync::Notify;

async fn home() -> (tempfile::TempDir, Bots, Notify) {
    let dir = tempfile::tempdir().unwrap();
    let bots = Bots::new(dir.path().to_path_buf(), String::new(), String::new());
    bots.create(&json!({ "name": "Ada", "prompt": "Be brief." }))
        .await
        .unwrap();
    bots.save("ada", &json!({ "mainThreadId": "thread-ada" }), false)
        .await
        .unwrap();
    (dir, bots, Notify::new())
}

fn deps<'a>(bots: &'a Bots, roster: &'a [Value], notify: &'a Notify) -> Deps<'a> {
    Deps {
        bots,
        roster,
        provision_now: notify,
        settings: json!({}),
    }
}

fn selection() -> Value {
    json!({
        "instanceId": "claudeAgent",
        "model": "claude-haiku-5-5",
        "options": [{ "id": "effort", "value": "low" }]
    })
}

#[tokio::test]
async fn turned_off_helpers_refuse_create_and_the_list_reports_the_policy() {
    let (_dir, bots, notify) = home().await;
    bots.save("ada", &json!({ "subagents": { "enabled": false } }), false)
        .await
        .unwrap();
    let roster = bots.list().await.unwrap();
    let d = deps(&bots, &roster, &notify);
    let err = dispatch(
        &d,
        "subagent_create",
        &json!({ "threadId": "thread-ada", "prompt": "Look things up." }),
    )
    .await
    .unwrap_err();
    assert_eq!(err, HELPERS_OFF);
    assert!(bots.get("ada-junior-1").await.unwrap().is_none());
    let listed = dispatch(&d, "subagent_list", &json!({ "threadId": "thread-ada" }))
        .await
        .unwrap();
    assert!(listed["subagents"].as_array().unwrap().is_empty());
    assert_eq!(listed["policy"]["enabled"], false);
    assert_eq!(listed["policy"]["count"], 0);
    assert_eq!(listed["policy"]["maxCount"], 8);
}

#[tokio::test]
async fn a_full_helper_list_refuses_the_next_create_and_still_lists() {
    let (_dir, bots, notify) = home().await;
    bots.save("ada", &json!({ "subagents": { "maxCount": 1 } }), false)
        .await
        .unwrap();
    let roster = bots.list().await.unwrap();
    dispatch(
        &deps(&bots, &roster, &notify),
        "subagent_create",
        &json!({ "threadId": "thread-ada", "prompt": "Look things up.", "name": "Scout" }),
    )
    .await
    .unwrap();
    let roster = bots.list().await.unwrap();
    let err = dispatch(
        &deps(&bots, &roster, &notify),
        "subagent_create",
        &json!({ "threadId": "thread-ada", "prompt": "Another." }),
    )
    .await
    .unwrap_err();
    assert_eq!(err, at_limit_message(1));
    let listed = dispatch(
        &deps(&bots, &roster, &notify),
        "subagent_list",
        &json!({ "threadId": "thread-ada" }),
    )
    .await
    .unwrap();
    assert_eq!(listed["subagents"].as_array().unwrap().len(), 1);
    assert_eq!(listed["subagents"][0]["slug"], "scout");
    assert_eq!(listed["policy"]["count"], 1);
    assert_eq!(listed["policy"]["maxCount"], 1);
    assert_eq!(listed["policy"]["enabled"], true);
}

#[tokio::test]
async fn omitted_fields_take_the_policy_defaults_and_a_sent_flag_wins() {
    let (_dir, bots, notify) = home().await;
    bots.save(
        "ada",
        &json!({
            "subagents": {
                "defaults": {
                    "inheritParentPrompt": true,
                    "toolAllow": ["read"],
                    "model": selection(),
                    "runtimeMode": "approval-required"
                }
            }
        }),
        false,
    )
    .await
    .unwrap();
    bots.save("ada", &json!({ "runtimeMode": "full-access" }), false)
        .await
        .unwrap();
    let roster = bots.list().await.unwrap();
    dispatch(
        &deps(&bots, &roster, &notify),
        "subagent_create",
        &json!({
            "threadId": "thread-ada",
            "prompt": "Look things up.",
            "name": "Scout",
            "toolAllow": ["write"],
            "model": "opus"
        }),
    )
    .await
    .unwrap();
    let scout = bots.get("scout").await.unwrap().unwrap();
    assert_eq!(scout["subagent"]["inheritParentPrompt"], true);
    assert_eq!(scout["subagent"]["toolAllow"], json!(["read"]));
    assert_eq!(scout["model"]["model"], "claude-haiku-5-5");
    assert_eq!(scout["model"]["instanceId"], "claudeAgent");
    assert_eq!(scout["runtimeMode"], "approval-required");
    assert_eq!(
        bots.get("ada").await.unwrap().unwrap()["runtimeMode"],
        "full-access"
    );

    let roster = bots.list().await.unwrap();
    dispatch(
        &deps(&bots, &roster, &notify),
        "subagent_create",
        &json!({
            "threadId": "thread-ada",
            "prompt": "Notes.",
            "name": "Notes",
            "inheritParentPrompt": false
        }),
    )
    .await
    .unwrap();
    let notes = bots.get("notes").await.unwrap().unwrap();
    assert_eq!(notes["subagent"]["inheritParentPrompt"], false);
    assert_eq!(notes["subagent"]["toolAllow"], json!(["read"]));

    let listed = dispatch(
        &deps(&bots, &roster, &notify),
        "subagent_list",
        &json!({ "threadId": "thread-ada" }),
    )
    .await
    .unwrap();
    let row = listed["subagents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["slug"] == "scout")
        .unwrap();
    assert_eq!(row["runtimeMode"], "approval-required");
    assert_eq!(row["model"]["model"], "claude-haiku-5-5");
    assert_eq!(row["toolAllow"], json!(["read"]));
    assert!(row.get("options").is_none());
    assert!(listed["policy"]["when"]
        .as_str()
        .unwrap()
        .contains("outlive this turn"));
}

#[tokio::test]
async fn a_settings_cap_does_not_store_a_mode_outside_the_allowlist() {
    let (_dir, bots, notify) = home().await;
    bots.save(
        "ada",
        &json!({
            "subagents": {
                "defaults": { "runtimeMode": "full-access" },
                "allowlist": { "runtimeModes": ["full-access"] }
            }
        }),
        false,
    )
    .await
    .unwrap();
    let roster = bots.list().await.unwrap();
    let mut d = deps(&bots, &roster, &notify);
    d.settings = json!({ "maxRuntimeMode": "auto" });
    dispatch(
        &d,
        "subagent_create",
        &json!({ "threadId": "thread-ada", "prompt": "Look things up." }),
    )
    .await
    .unwrap();
    assert_eq!(
        bots.get("ada-junior-1").await.unwrap().unwrap()["runtimeMode"],
        "full-access"
    );
}

#[test]
fn the_create_schema_follows_the_helper_policy() {
    let defs = tool_defs();
    assert!(defs.iter().any(|tool| tool["name"] == "subagent_list"));
    let create = defs
        .iter()
        .find(|tool| tool["name"] == "subagent_create")
        .unwrap();
    let text = create.to_string();
    assert!(text.contains("turned off"), "{text}");
    assert!(text.contains("owner's default"), "{text}");
    assert!(!text.contains("Default false"), "{text}");
    assert_eq!(
        create["inputSchema"]["required"],
        json!(["threadId", "prompt"])
    );
}
