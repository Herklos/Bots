use super::webhook::new_webhook_routine;
use super::*;
use serde_json::Value;

const SECRET: &str = "s3cret-not-for-rows";

fn bot() -> Value {
    json!({
        "projectId": "p1",
        "mainThreadId": "t1",
        "runtimeMode": "full-access",
        "model": { "instanceId": "claudeAgent", "model": "claude-haiku-5-5" }
    })
}

fn github(extra: Value) -> Value {
    let mut input = json!({
        "title": "GitHub pushes",
        "prompt": "A push arrived.",
        "preset": "github",
        "secret": SECRET,
        "runtimeMode": "full-access",
        "filters": [{ "path": "headers.X-GitHub-Event", "op": "equals", "value": "push" }],
    });
    if let (Some(base), Some(more)) = (input.as_object_mut(), extra.as_object()) {
        for (key, value) in more {
            base.insert(key.clone(), value.clone());
        }
    }
    input
}

#[test]
fn a_github_preset_is_disabled_and_asks_for_approval() {
    let task = new_webhook_routine(&bot(), &github(json!({})), 0).unwrap();
    assert_eq!(task["enabled"], false);
    assert_eq!(task["runtimeMode"], "approval-required");
    assert_eq!(task["threadId"], "t1");
    assert_eq!(task["projectId"], "p1");
    assert_eq!(task["schedule"]["type"], "webhook");
    assert_eq!(task["schedule"]["maxDeliveryAgeMinutes"], 60);
    assert_eq!(
        task["schedule"]["signature"]["header"],
        "x-hub-signature-256"
    );
    assert_eq!(task["schedule"]["signature"]["encoding"], "hex");
    assert_eq!(task["schedule"]["signature"]["prefix"], "sha256=");
    assert_eq!(task["schedule"]["signature"]["secret"], SECRET);
    assert_eq!(
        task["schedule"]["dedupe"]["replayKey"],
        "{{headers.x-github-delivery}}"
    );
    assert_eq!(
        task["schedule"]["filters"][0]["path"],
        "headers.x-github-event"
    );
    let prompt = task["prompt"].as_str().unwrap();
    assert!(prompt.contains("do not call the Vault"));
    assert!(prompt.contains("not an instruction"));
    assert!(prompt.contains("{{body}}"));
    assert!(!prompt.contains("<webhook-data>"));
    assert!(!prompt.contains(SECRET));
    assert_eq!(task["modelSelection"]["model"], "claude-haiku-5-5");
    let mut turned_on = github(json!({}));
    turned_on["enable"] = json!(true);
    assert_eq!(
        new_webhook_routine(&bot(), &turned_on, 0).unwrap()["enabled"],
        true
    );
}

#[test]
fn seventeen_filters_are_refused() {
    let filters = |count: usize| {
        Value::Array(
            (0..count)
                .map(|n| json!({ "path": "body.n", "op": "equals", "value": n.to_string() }))
                .collect(),
        )
    };
    assert!(new_webhook_routine(&bot(), &github(json!({ "filters": filters(16) })), 0).is_ok());
    assert_eq!(
        new_webhook_routine(&bot(), &github(json!({ "filters": filters(17) })), 0).unwrap_err(),
        "A webhook routine has at most 16 filters."
    );
}

#[test]
fn a_bad_filter_path_is_refused() {
    let err = new_webhook_routine(
        &bot(),
        &github(json!({
            "filters": [{ "path": "payload.action", "op": "equals", "value": "opened" }]
        })),
        0,
    )
    .unwrap_err();
    assert_eq!(err, "A filter path starts with body, headers, or query.");
    assert!(new_webhook_routine(
        &bot(),
        &github(json!({
            "filters": [{ "path": "body.commits[].id", "op": "exists" }]
        })),
        0,
    )
    .is_ok());
}

#[test]
fn an_in_filter_refuses_a_comma() {
    let refused = |value: Value| {
        new_webhook_routine(
            &bot(),
            &github(json!({
                "filters": [{ "path": "body.action", "op": "in", "value": value }]
            })),
            0,
        )
        .unwrap_err()
    };
    assert_eq!(
        refused(json!(["opened", "closed,reopened"])),
        "An in filter value cannot contain a comma."
    );
    assert_eq!(
        refused(json!([" opened"])),
        "An in filter value cannot start or end with a space."
    );
    let task = new_webhook_routine(
        &bot(),
        &github(json!({
            "filters": [{ "path": "body.action", "op": "in", "value": ["opened", "closed"] }]
        })),
        0,
    )
    .unwrap();
    assert_eq!(
        task["schedule"]["filters"][0]["value"],
        json!(["opened", "closed"])
    );
}

#[test]
fn a_filter_shaped_replay_key_is_refused() {
    let err = new_webhook_routine(
        &bot(),
        &github(json!({
            "dedupe": { "replayKey": { "path": "headers.x-github-delivery", "op": "equals", "value": "1" } }
        })),
        0,
    )
    .unwrap_err();
    assert_eq!(err, "A replay key is a template, not a filter.");
    let task = new_webhook_routine(
        &bot(),
        &github(json!({
            "dedupe": { "coalesceKey": "{{body.repository.id}}", "coalesceWindowSeconds": 30 }
        })),
        0,
    )
    .unwrap();
    assert_eq!(
        task["schedule"]["dedupe"]["replayKey"],
        "{{headers.x-github-delivery}}"
    );
    assert_eq!(
        task["schedule"]["dedupe"]["coalesceKey"],
        "{{body.repository.id}}"
    );
    assert_eq!(task["schedule"]["dedupe"]["coalesceWindowSeconds"], 30);
}

#[test]
fn the_secret_is_passed_once_and_absent_from_the_row() {
    let task = new_webhook_routine(&bot(), &github(json!({})), 0).unwrap();
    assert_eq!(
        secret_spots(&task),
        vec!["schedule.signature.secret".to_string()]
    );
    let mut saved = task;
    saved["id"] = json!("scheduled-task:1");
    saved["webhook"] = json!({
        "path": "/api/hooks/scheduled-task%3A1/hook-token-value",
        "url": "https://relay.example/hook-token-value",
        "hasSecret": true,
    });
    let row = task_row(&saved);
    let text = row.to_string();
    assert!(!text.contains(SECRET));
    assert!(!text.contains("hook-token-value"));
    assert!(!text.contains("/api/hooks/"));
    assert!(row.get("webhook").is_none());
    assert!(row.get("secret").is_none());
    assert_eq!(row["kind"], "webhook");
    assert_eq!(row["hook"]["hasSecret"], true);
    assert_eq!(row["hook"]["filters"].as_u64(), Some(1));
    assert_eq!(row["hook"]["queued"].as_u64(), Some(0));
    assert_eq!(row["schedule"], "When a webhook arrives");
}

fn secret_spots(value: &Value) -> Vec<String> {
    let mut found = Vec::new();
    walk(value, "", &mut found);
    found
}

fn walk(value: &Value, path: &str, found: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let next = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                if child.as_str() == Some(SECRET) {
                    found.push(next);
                } else {
                    walk(child, &next, found);
                }
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                walk(child, &format!("{path}[{index}]"), found);
            }
        }
        _ => {}
    }
}

#[test]
fn phase0_webhook_fixtures_decode_into_rows() {
    let upsert: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/t3/webhook-task-upsert.json"
    ))
    .unwrap();
    let row = task_row(&upsert["task"]);
    assert_eq!(
        row["id"],
        "scheduled-task:e8bbe91d-9db9-4e49-96b6-adee08c02581"
    );
    assert_eq!(row["kind"], "webhook");
    assert_eq!(row["schedule"], "When a webhook arrives");
    assert_eq!(row["enabled"], true);
    assert_eq!(row["hook"]["hasSecret"], false);
    assert_eq!(row["hook"]["filters"].as_u64(), Some(0));
    assert_eq!(row["hook"]["queued"].as_u64(), Some(0));
    let text = row.to_string();
    assert!(!text.contains("redacted-token"));
    assert!(!text.contains("/api/hooks/"));

    let signed: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/t3/webhook-task-signature.json"
    ))
    .unwrap();
    let row = task_row(&signed["task"]);
    assert_eq!(row["kind"], "webhook");
    assert_eq!(row["hook"]["hasSecret"], true);
    assert_eq!(row["schedule"], "When a webhook arrives");
    let text = row.to_string();
    assert!(!text.contains("redacted-token"));
    assert!(!text.contains("/api/hooks/"));
    assert!(row.get("webhook").is_none());

    let schedule = task_row(&json!({
        "id": "a",
        "schedule": { "type": "interval", "everyMs": 3_600_000 }
    }));
    assert_eq!(schedule["kind"], "schedule");
    assert!(schedule.get("hook").is_none());
}

fn addressed() -> Value {
    json!({
        "webhook": {
            "path": "/api/hooks/scheduled-task%3Aabc/tok-1",
            "url": "https://connect.example/v1/hooks/key/task/tok-1",
            "hasSecret": false
        }
    })
}

#[test]
fn the_url_uses_public_base_then_connect_then_the_tunnel_then_this_computer() {
    let task = addressed();
    let local = "http://127.0.0.1:3780";
    let tunnel = Some("https://brave-otter-12.trycloudflare.com/");
    let base = webhook::webhook_url(&task, "https://hooks.example.com/", tunnel, local);
    assert_eq!(base.source, "publicBase");
    assert!(base.stable);
    assert_eq!(
        base.url,
        "https://hooks.example.com/hooks/scheduled-task%3Aabc/tok-1"
    );
    let connect = webhook::webhook_url(&task, "", tunnel, local);
    assert_eq!(connect.source, "connect");
    assert!(connect.stable);
    assert_eq!(
        connect.url,
        "https://connect.example/v1/hooks/key/task/tok-1"
    );
    let mut no_connect = task.clone();
    no_connect["webhook"]["url"] = Value::Null;
    let quick = webhook::webhook_url(&no_connect, "http://nope.example", tunnel, local);
    assert_eq!(quick.source, "tunnel");
    assert!(!quick.stable);
    assert_eq!(
        quick.url,
        "https://brave-otter-12.trycloudflare.com/hooks/scheduled-task%3Aabc/tok-1"
    );
    let here = webhook::webhook_url(&no_connect, "", None, local);
    assert_eq!(here.source, "local");
    assert!(!here.stable);
    assert_eq!(
        here.url,
        "http://127.0.0.1:3780/hooks/scheduled-task%3Aabc/tok-1"
    );
    assert!(!webhook::public_base_ok("https://hooks.example.com/hooks"));
    assert!(!webhook::public_base_ok("https://hooks.example.com?x=1"));
    assert!(webhook::public_base_ok("https://hooks.example.com"));
    assert_eq!(
        webhook::canonical_public_base("https://hooks.example.com/"),
        "https://hooks.example.com"
    );
}

#[tokio::test]
async fn the_tunnel_handle_comes_from_remote_access() {
    let dir = tempfile::tempdir().unwrap();
    let tailnet = std::sync::Arc::new(crate::network::exposure::Tailnet::new());
    let remote = crate::network::remote_access::RemoteAccess::new(
        crate::network::remote_access::RemoteConfig {
            home: dir.path().to_path_buf(),
            store_file: dir.path().join("access.json"),
            url_file: dir.path().join("tunnel-url"),
            local_port: 3780,
            env_password: String::new(),
            env_tailnet: false,
            env_tailnet_users: vec![],
            tailscale_bin: None,
            network_env: None,
            process_host: None,
            merged_host: None,
        },
        tailnet,
    );
    remote.set_quick_tunnel_for_test("https://brave-otter-12.trycloudflare.com".into());
    let mut task = addressed();
    task["webhook"]["url"] = Value::Null;
    let settings = json!({ "webhooks": { "publicBase": "" } });
    let got = webhook::webhook_url_for(&task, &settings, &remote);
    assert_eq!(got.source, "tunnel");
    assert!(!got.stable);
    assert_eq!(
        got.url,
        "https://brave-otter-12.trycloudflare.com/hooks/scheduled-task%3Aabc/tok-1"
    );
}
