//! `GET /api/providers/limits` reads the same cached harness config as the model list.
use super::tests::settings_state_with;
use super::*;
use crate::harness::adapters::mock::MockHarness;
use crate::harness::Harness;

fn config() -> Value {
    json!({
        "settings": { "providerModelPreferences": {} },
        "providers": [
            {
                "instanceId": "claude",
                "driver": "claude",
                "enabled": true,
                "installed": true,
                "status": "ready",
                "displayName": "Claude",
                "auth": {
                    "status": "authenticated",
                    "label": "Work",
                    "email": "person@example.com"
                },
                "runtimePaths": { "home": "/secret/runtime/path" },
                "models": [{ "slug": "claude-haiku-5-5", "name": "Claude Haiku" }],
                "usageLimits": {
                    "checkedAt": "2026-11-12T14:00:00Z",
                    "credentialFingerprint": "fp-secret-9f3a",
                    "resetCredits": [{ "id": "credit-secret-11" }],
                    "windows": [{
                        "id": "5h",
                        "kind": "session",
                        "label": "5 hour",
                        "usedPercent": 62,
                        "resetsAt": "2026-11-12T17:00:00Z"
                    }]
                }
            },
            {
                "instanceId": "cursor",
                "driver": "cursor",
                "enabled": false,
                "installed": false,
                "status": "disabled",
                "displayName": "Cursor",
                "auth": { "status": "unknown" }
            }
        ]
    })
}

fn leaks(body: &Value) -> bool {
    let text = body.to_string();
    [
        "person@example.com",
        "fp-secret-9f3a",
        "credit-secret-11",
        "/secret/runtime/path",
        "credentialFingerprint",
        "resetCredits",
        "runtimePaths",
    ]
    .iter()
    .any(|secret| text.contains(secret))
}

#[tokio::test]
async fn provider_limits_reuse_the_model_list_cache() {
    let dir = tempfile::tempdir().unwrap();
    let mock = Arc::new(MockHarness::new());
    mock.set_provider_config(config());
    let state = settings_state_with(dir.path(), Some(Arc::clone(&mock) as Arc<dyn Harness>));

    let Json(models) = providers_get(State(state.clone())).await.unwrap();
    let Json(limits) = providers_limits(State(state.clone())).await.unwrap();
    let Json(again) = providers_limits(State(state)).await.unwrap();

    assert_eq!(mock.provider_config_reads(), 1);
    assert_eq!(again["at"], limits["at"]);
    assert!(limits["at"].as_str().unwrap().ends_with('Z'));

    let offered = models["providers"].as_array().unwrap();
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0]["instanceId"], "claude");
    assert_eq!(offered[0]["account"], "Work");
    assert!(!leaks(&models));

    let bars = limits["providers"].as_array().unwrap();
    assert_eq!(bars.len(), 2);
    assert_eq!(bars[0]["instanceId"], "claude");
    assert_eq!(bars[0]["windows"][0]["usedPercent"], json!(62));
    assert_eq!(bars[0]["windows"][0]["resetsAt"], "2026-11-12T17:00:00Z");
    assert_eq!(bars[1]["instanceId"], "cursor");
    assert_eq!(bars[1]["state"], "not-installed");
    assert_eq!(bars[1]["windows"], json!([]));
    assert!(!leaks(&limits));
    assert!(!leaks(&again));
}

#[tokio::test]
async fn provider_limits_report_a_failed_read() {
    let dir = tempfile::tempdir().unwrap();
    let mock = Arc::new(MockHarness::new());
    let state = settings_state_with(dir.path(), Some(Arc::clone(&mock) as Arc<dyn Harness>));
    let err = providers_limits(State(state)).await.unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_GATEWAY);
    assert!(err.message.contains("provider limits"));
}

#[tokio::test]
async fn provider_refresh_is_rate_limited_without_sleep() {
    let dir = tempfile::tempdir().unwrap();
    let mock = Arc::new(MockHarness::new());
    mock.set_provider_config(config());
    let state = settings_state_with(dir.path(), Some(Arc::clone(&mock) as Arc<dyn Harness>));

    let Json(first) = providers_refresh(State(state.clone())).await.unwrap();
    let Json(second) = providers_refresh(State(state)).await.unwrap();

    assert_eq!(mock.provider_refresh_calls(), 1);
    assert_eq!(mock.provider_config_reads(), 1);
    assert_eq!(first["cooling"], json!(false));
    assert_eq!(second["cooling"], json!(true));
    assert_eq!(second["providers"][0]["instanceId"], "claude");
    assert!(!leaks(&first));
    assert!(!leaks(&second));
}
