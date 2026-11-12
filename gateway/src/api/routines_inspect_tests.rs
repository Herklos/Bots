use super::tests::settings_state_with;
use super::*;
use crate::harness::adapters::mock::MockHarness;
use crate::harness::Routines;

async fn ready() -> (tempfile::TempDir, Arc<AppState>, Arc<MockHarness>) {
    let dir = tempfile::tempdir().unwrap();
    let mock = Arc::new(MockHarness::new());
    let state = settings_state_with(
        dir.path(),
        Some(Arc::clone(&mock) as Arc<dyn crate::harness::Harness>),
    );
    crate::storage::write_json(
        &state.settings_path(),
        &json!({ "webhooks": { "publicBase": "https://hooks.example" } }),
    )
    .await
    .unwrap();
    state.bots.create(&json!({ "name": "Ada" })).await.unwrap();
    state
        .bots
        .save(
            "ada",
            &json!({ "mainThreadId": "ada-thread", "projectId": "p" }),
            false,
        )
        .await
        .unwrap();
    state.bots.create(&json!({ "name": "Bea" })).await.unwrap();
    state
        .bots
        .save("bea", &json!({ "mainThreadId": "bea-thread" }), false)
        .await
        .unwrap();
    (dir, state, mock)
}

fn token_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap()
}

#[tokio::test]
async fn hook_rotate_and_deliveries_stay_on_this_bot() {
    let (_dir, state, mock) = ready().await;
    let saved = mock
        .upsert(&json!({
            "title": "GitHub",
            "threadId": "ada-thread",
            "enabled": true,
            "schedule": {
                "type": "webhook",
                "signature": {
                    "header": "x-hub-signature-256",
                    "secret": "s3cret-not-for-rows"
                }
            }
        }))
        .await
        .unwrap();
    let id = saved["task"]["id"].as_str().unwrap().to_string();
    let path = saved["task"]["webhook"]["path"]
        .as_str()
        .unwrap()
        .to_string();
    let token = token_of(&path).to_string();
    mock.upsert(&json!({
        "id": "scheduled-task:morning",
        "title": "Morning",
        "threadId": "ada-thread",
        "enabled": true,
        "schedule": { "type": "interval" }
    }))
    .await
    .unwrap();
    mock.push_delivery(
        &id,
        json!({
            "id": "delivery:old",
            "receivedAt": "2026-11-12T20:00:00.000Z",
            "method": "POST",
            "bodyBytes": 4,
            "outcome": "filtered",
            "body": "older"
        }),
    );
    mock.push_delivery(
        &id,
        json!({
            "id": "delivery:1",
            "receivedAt": "2026-11-12T20:21:16.707Z",
            "method": "POST",
            "contentType": "application/json",
            "bodyBytes": 16,
            "outcome": "accepted",
            "headers": {
                "authorization": "[redacted]",
                "cookie": "[redacted]",
                "x-hub-signature-256": "sha256=abc",
                "x-custom-header": "FooBar"
            },
            "body": "{\"w0\":\"headers\"}",
            "renderedPrompt": "A webhook arrived."
        }),
    );

    let listed = bot_routines(State(state.clone()), Path("ada".into()))
        .await
        .unwrap()
        .0;
    let hook_row = listed["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == id)
        .unwrap();
    assert_eq!(hook_row["kind"], "webhook");
    assert_eq!(hook_row["hook"]["lastOutcome"], "accepted");
    assert!(hook_row.get("url").is_none());
    let listed_text = listed.to_string();
    assert!(!listed_text.contains(&token));
    assert!(!listed_text.contains("s3cret-not-for-rows"));

    let hook = bot_hook(State(state.clone()), Path(("ada".into(), id.clone())))
        .await
        .unwrap()
        .0;
    assert_eq!(hook["source"], "publicBase");
    assert_eq!(hook["stable"], true);
    assert!(hook["url"]
        .as_str()
        .unwrap()
        .starts_with("https://hooks.example/hooks/"));
    assert!(hook["url"].as_str().unwrap().contains(&token));
    assert!(!hook.to_string().contains("s3cret-not-for-rows"));

    let deliveries = bot_deliveries(State(state.clone()), Path(("ada".into(), id.clone())))
        .await
        .unwrap()
        .0;
    assert_eq!(deliveries["deliveries"][0]["id"], "delivery:1");
    assert_eq!(deliveries["deliveries"][0]["outcome"], "accepted");
    assert_eq!(deliveries["deliveries"].as_array().unwrap().len(), 2);
    let delivery_text = deliveries.to_string();
    assert!(!delivery_text.contains(&token));
    assert!(!delivery_text.contains("sha256=abc"));
    assert!(!delivery_text.contains("headers"));
    assert!(deliveries["deliveries"][0].get("body").is_none());

    let detail = bot_delivery(
        State(state.clone()),
        Path(("ada".into(), id.clone(), "delivery:1".into())),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(detail["headers"]["authorization"], "[hidden]");
    assert_eq!(detail["headers"]["cookie"], "[hidden]");
    assert_eq!(detail["headers"]["x-hub-signature-256"], "[hidden]");
    assert_eq!(detail["headers"]["x-custom-header"], "FooBar");
    assert_eq!(detail["renderedPrompt"], "A webhook arrived.");
    assert!(!detail.to_string().contains(&token));

    let rotated = bot_rotate(State(state.clone()), Path(("ada".into(), id.clone())))
        .await
        .unwrap()
        .0;
    assert_ne!(rotated["url"], hook["url"]);
    assert!(!rotated["url"].as_str().unwrap().contains(&token));
    assert_eq!(rotated["source"], "publicBase");
    assert_eq!(rotated["stable"], true);

    let missing = bot_hook(State(state.clone()), Path(("bea".into(), id.clone())))
        .await
        .unwrap_err();
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    let other = bot_hook(
        State(state.clone()),
        Path(("ada".into(), "scheduled-task:missing".into())),
    )
    .await
    .unwrap_err();
    assert_eq!(other.status, StatusCode::NOT_FOUND);
    let plain = bot_deliveries(
        State(state.clone()),
        Path(("ada".into(), "scheduled-task:morning".into())),
    )
    .await
    .unwrap_err();
    assert_eq!(plain.status, StatusCode::BAD_REQUEST);
    assert_eq!(plain.message, "That routine is not a webhook.");
    let gone = bot_delivery(
        State(state),
        Path(("ada".into(), id, "delivery:missing".into())),
    )
    .await
    .unwrap_err();
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
    assert_eq!(gone.message, "That delivery is not on this webhook.");
}
