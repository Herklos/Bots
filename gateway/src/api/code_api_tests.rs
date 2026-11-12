use super::tests::{settings_state, settings_state_with};
use super::*;
use crate::harness::adapters::mock::MockHarness;

fn headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    headers
}

async fn put(state: &Arc<AppState>, body: Value) -> Result<Value, ApiError> {
    put_settings(
        State(state.clone()),
        headers(),
        Bytes::from(body.to_string()),
    )
    .await
    .map(|Json(value)| value)
}

fn state_with(dir: &std::path::Path, is_repo: bool) -> (Arc<AppState>, Arc<MockHarness>) {
    let mock = Arc::new(MockHarness::new());
    mock.set_is_repo(is_repo);
    let state = settings_state_with(
        dir,
        Some(Arc::clone(&mock) as Arc<dyn crate::harness::Harness>),
    );
    (state, mock)
}

async fn ada(state: &AppState) {
    state.bots.create(&json!({ "name": "Ada" })).await.unwrap();
}

#[tokio::test]
async fn beta_code_defaults_off_and_stays_a_boolean() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    assert_eq!(state.settings().await["beta"]["code"], false);
    assert_eq!(state.settings().await["beta"]["projects"], false);

    let unknown = put(&state, json!({ "beta": { "code": true, "nope": true } }))
        .await
        .unwrap_err();
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST);
    assert_eq!(unknown.message, "Unknown beta feature: nope");

    let bad = put(&state, json!({ "beta": { "code": "yes" } }))
        .await
        .unwrap_err();
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert_eq!(bad.message, "beta.code must be true or false");
    assert_eq!(state.settings().await["beta"]["code"], false);

    let next = put(&state, json!({ "beta": { "code": true } }))
        .await
        .unwrap();
    assert_eq!(next["beta"]["code"], true);
    assert_eq!(next["beta"]["projects"], false);
    let kept = put(&state, json!({ "userName": "Ada" })).await.unwrap();
    assert_eq!(kept["userName"], "Ada");
    assert_eq!(kept["beta"]["code"], true);
    assert_eq!(state.settings().await["beta"]["code"], true);
}

#[tokio::test]
async fn the_route_is_disabled_and_makes_no_harness_call() {
    let dir = tempfile::tempdir().unwrap();
    let (state, mock) = state_with(dir.path(), true);
    ada(&state).await;
    let Json(body) = code_get(State(state), Path("ada".into())).await.unwrap();
    assert_eq!(body, json!({ "enabled": false }));
    assert_eq!(mock.repo_calls(), 0);
}

#[tokio::test]
async fn the_route_reports_auto_on_and_off_when_the_switch_is_on() {
    let dir = tempfile::tempdir().unwrap();
    let (state, mock) = state_with(dir.path(), true);
    ada(&state).await;
    put(&state, json!({ "beta": { "code": true } }))
        .await
        .unwrap();

    let Json(auto) = code_get(State(state.clone()), Path("ada".into()))
        .await
        .unwrap();
    assert_eq!(auto, json!({ "enabled": true, "signal": true }));
    assert_eq!(mock.repo_calls(), 1);
    let Json(cached) = code_get(State(state.clone()), Path("ada".into()))
        .await
        .unwrap();
    assert_eq!(cached["signal"], true);
    assert_eq!(mock.repo_calls(), 1);

    state
        .bots
        .save("ada", &json!({ "code": "off" }), false)
        .await
        .unwrap();
    let Json(off) = code_get(State(state.clone()), Path("ada".into()))
        .await
        .unwrap();
    assert_eq!(off, json!({ "enabled": true, "signal": false }));
    assert_eq!(mock.repo_calls(), 1);

    state
        .bots
        .save("ada", &json!({ "code": "on" }), false)
        .await
        .unwrap();
    mock.set_is_repo(false);
    let Json(on) = code_get(State(state), Path("ada".into())).await.unwrap();
    assert_eq!(on, json!({ "enabled": true, "signal": true }));
    assert_eq!(mock.repo_calls(), 1);
}

#[tokio::test]
async fn auto_without_a_repository_stays_hidden() {
    let dir = tempfile::tempdir().unwrap();
    let (state, mock) = state_with(dir.path(), false);
    ada(&state).await;
    put(&state, json!({ "beta": { "code": true } }))
        .await
        .unwrap();
    let Json(body) = code_get(State(state), Path("ada".into())).await.unwrap();
    assert_eq!(body, json!({ "enabled": true, "signal": false }));
    assert_eq!(mock.repo_calls(), 1);
}

#[tokio::test]
async fn a_missing_bot_is_not_found_and_makes_no_harness_call() {
    let dir = tempfile::tempdir().unwrap();
    let (state, mock) = state_with(dir.path(), true);
    let err = code_get(State(state), Path("ada".into()))
        .await
        .unwrap_err();
    assert_eq!(err.status, StatusCode::NOT_FOUND);
    assert_eq!(mock.repo_calls(), 0);
}
