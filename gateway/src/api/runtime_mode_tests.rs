use super::tests::{json_headers, put, settings_state};
use super::*;

#[tokio::test]
async fn settings_accept_independent_and_reject_garbage_without_changing_the_default() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    assert!(AppState::settings_defaults()
        .get("defaultRuntimeMode")
        .is_none());
    assert_eq!(state.settings().await["maxRuntimeMode"], "full-access");
    let next = put(&state, json!({ "defaultRuntimeMode": "independent" }))
        .await
        .unwrap();
    assert_eq!(next["defaultRuntimeMode"], "independent");
    assert_eq!(next["maxRuntimeMode"], "full-access");
    assert_eq!(
        storage::read_json(&state.settings_path()).await.unwrap()["defaultRuntimeMode"],
        "independent"
    );
    let err = put(&state, json!({ "defaultRuntimeMode": "ask" }))
        .await
        .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        err.message,
        "defaultRuntimeMode must be approval-required, auto, full-access, or independent"
    );
    let err = put(&state, json!({ "maxRuntimeMode": "independent" }))
        .await
        .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    let Json(ada) = create_bot(
        State(state.clone()),
        json_headers(),
        Bytes::from(r#"{"name":"Ada","runtimeMode":"independent"}"#),
    )
    .await
    .unwrap();
    assert_eq!(ada["runtimeMode"], "independent");
    let refused = save_bot(
        State(state.clone()),
        Path("ada".into()),
        json_headers(),
        Bytes::from(r#"{"runtimeMode":"nope"}"#),
    )
    .await
    .unwrap_err();
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        state.bots.get("ada").await.unwrap().unwrap()["runtimeMode"],
        "independent"
    );
}
