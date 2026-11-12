//! Owner model and approval edits on a durable child, clamped to the parent allowlist.
use super::tests::settings_state_with;
use super::*;
use crate::bots::helpers_policy::{MODEL_DENIED, MODE_DENIED};
use crate::harness::adapters::mock::MockHarness;
use crate::harness::Harness;

fn headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    headers
}

async fn put_model(state: &Arc<AppState>, slug: &str, body: &Value) -> Result<Value, ApiError> {
    model_put(
        State(state.clone()),
        Path(slug.to_string()),
        headers(),
        Bytes::from(body.to_string()),
    )
    .await
    .map(|Json(value)| value)
}

fn state() -> (tempfile::TempDir, Arc<AppState>) {
    let dir = tempfile::tempdir().unwrap();
    let mock = Arc::new(MockHarness::new());
    let state = settings_state_with(dir.path(), Some(Arc::clone(&mock) as Arc<dyn Harness>));
    (dir, state)
}

async fn child(state: &AppState, allowlist: Value) -> String {
    state.bots.create(&json!({ "name": "Ada" })).await.unwrap();
    state
        .bots
        .save(
            "ada",
            &json!({ "subagents": { "allowlist": allowlist } }),
            false,
        )
        .await
        .unwrap();
    state
        .bots
        .create(&json!({ "parentSlug": "ada", "name": "Scout" }))
        .await
        .unwrap()["slug"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn u19_an_allowlisted_model_is_saved() {
    let (_dir, state) = state();
    let slug = child(
        &state,
        json!({
            "models": [
                { "instanceId": "claudeAgent", "model": "claude-haiku-5-5" },
                { "instanceId": "claudeAgent", "model": "claude-sonnet-4-6" }
            ]
        }),
    )
    .await;
    let saved = put_model(
        &state,
        &slug,
        &json!({ "instanceId": "claudeAgent", "model": "claude-sonnet-4-6" }),
    )
    .await
    .unwrap();
    assert_eq!(saved["ok"], true);
    let stored = state.bots.get(&slug).await.unwrap().unwrap();
    assert_eq!(stored["model"]["model"], "claude-sonnet-4-6");
    assert_eq!(stored["model"]["instanceId"], "claudeAgent");
}

#[tokio::test]
async fn u20_a_model_outside_the_allowlist_is_rejected() {
    let (_dir, state) = state();
    let slug = child(
        &state,
        json!({ "models": [{ "instanceId": "claudeAgent", "model": "claude-haiku-5-5" }] }),
    )
    .await;
    let err = put_model(
        &state,
        &slug,
        &json!({ "instanceId": "claudeAgent", "model": "claude-sonnet-4-6" }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert_eq!(err.message, MODEL_DENIED);
    assert!(state.bots.get(&slug).await.unwrap().unwrap()["model"].is_null());
}

#[tokio::test]
async fn u21_a_runtime_mode_outside_the_allowlist_is_rejected() {
    let (_dir, state) = state();
    let slug = child(&state, json!({ "runtimeModes": ["auto"] })).await;
    let err = put_model(
        &state,
        &slug,
        &json!({
            "instanceId": "claudeAgent",
            "model": "claude-haiku-5-5",
            "runtimeMode": "full-access"
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert_eq!(err.message, MODE_DENIED);
    let stored = state.bots.get(&slug).await.unwrap().unwrap();
    assert!(stored.get("runtimeMode").is_none());
    assert!(stored["model"].is_null());
}
