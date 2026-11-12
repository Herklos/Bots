use super::tests::{put, settings_state};
use super::*;

#[tokio::test]
async fn webhooks_public_base_defaults_empty_and_rejects_unknown_keys() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    assert_eq!(
        AppState::settings_defaults()["webhooks"]["publicBase"],
        json!("")
    );
    assert_eq!(state.settings().await["webhooks"]["publicBase"], "");
    storage::write_json(&state.settings_path(), &json!({ "userName": "Ada" }))
        .await
        .unwrap();
    assert_eq!(state.settings().await["userName"], "Ada");
    assert_eq!(state.settings().await["webhooks"]["publicBase"], "");

    let unknown = put(
        &state,
        json!({ "webhooks": { "nope": "https://example.com" } }),
    )
    .await
    .unwrap_err();
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST);
    assert!(unknown.message.contains("Unknown webhooks setting"));
    let http = put(
        &state,
        json!({ "webhooks": { "publicBase": "http://hooks.example.com" } }),
    )
    .await
    .unwrap_err();
    assert_eq!(http.status, StatusCode::BAD_REQUEST);
    let path = put(
        &state,
        json!({ "webhooks": { "publicBase": "https://hooks.example.com/hooks" } }),
    )
    .await
    .unwrap_err();
    assert_eq!(path.status, StatusCode::BAD_REQUEST);
    let query = put(
        &state,
        json!({ "webhooks": { "publicBase": "https://hooks.example.com?x=1" } }),
    )
    .await
    .unwrap_err();
    assert_eq!(query.status, StatusCode::BAD_REQUEST);
    let on_disk = storage::read_json(&state.settings_path()).await.unwrap();
    assert!(on_disk.get("webhooks").is_none());
    assert_eq!(on_disk["userName"], "Ada");

    let next = put(
        &state,
        json!({ "webhooks": { "publicBase": "https://hooks.example.com/" } }),
    )
    .await
    .unwrap();
    assert_eq!(next["webhooks"]["publicBase"], "https://hooks.example.com");
    assert_eq!(next["userName"], "Ada");
    let Json(view) = get_settings(State(Arc::clone(&state))).await;
    assert_eq!(view["webhooks"]["publicBase"], "https://hooks.example.com");

    let kept = put(&state, json!({ "userName": "Bea" })).await.unwrap();
    assert_eq!(kept["userName"], "Bea");
    assert_eq!(kept["webhooks"]["publicBase"], "https://hooks.example.com");
    let cleared = put(&state, json!({ "webhooks": { "publicBase": "" } }))
        .await
        .unwrap();
    assert_eq!(cleared["webhooks"]["publicBase"], "");
}
