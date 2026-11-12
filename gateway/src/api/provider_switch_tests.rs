//! The mock harness records each model it is told to apply. A provider switch
//! reaches it only on the confirmed request.
use super::tests::settings_state_with;
use super::*;
use crate::bots::provider_switch::CONFIRM_SWITCH;
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

async fn ada(state: &AppState, model: Value) -> String {
    let created = state
        .bots
        .create(&json!({ "name": "Ada", "model": model }))
        .await
        .unwrap();
    let slug = created["slug"].as_str().unwrap().to_string();
    state
        .bots
        .save(
            &slug,
            &json!({ "projectId": "project-ada", "mainThreadId": "thread-ada" }),
            false,
        )
        .await
        .unwrap();
    slug
}

fn harness(mock: &Arc<MockHarness>) -> Option<Arc<dyn Harness>> {
    Some(Arc::clone(mock) as Arc<dyn Harness>)
}

#[tokio::test]
async fn a_provider_switch_is_a_separate_confirmed_step() {
    let dir = tempfile::tempdir().unwrap();
    let mock = Arc::new(MockHarness::new());
    let state = settings_state_with(dir.path(), harness(&mock));
    let slug = ada(
        &state,
        json!({
            "instanceId": "claudeAgent",
            "model": "claude-haiku-5-5",
            "options": [{ "id": "effort", "value": "medium" }],
        }),
    )
    .await;

    let switch = json!({
        "instanceId": "codex",
        "model": "gpt-5.4",
        "runtimeMode": "full-access",
    });
    let held = put_model(&state, &slug, &switch).await.unwrap_err();
    assert_eq!(held.status, StatusCode::CONFLICT);
    assert_eq!(held.message, CONFIRM_SWITCH);
    let mut worded = switch.clone();
    worded["confirmSwitch"] = json!("true");
    let still_held = put_model(&state, &slug, &worded).await.unwrap_err();
    assert_eq!(still_held.status, StatusCode::CONFLICT);

    let bot = state.bots.get(&slug).await.unwrap().unwrap();
    assert_eq!(bot["model"]["instanceId"], "claudeAgent");
    assert_eq!(bot["model"]["model"], "claude-haiku-5-5");
    assert!(bot.get("runtimeMode").is_none());
    assert!(
        mock.model_pushes().is_empty(),
        "an unconfirmed switch is not sent to the harness"
    );
    assert!(
        storage::read_json(
            &state
                .cfg
                .bots_root
                .join(".botapp")
                .join("provider-switches.json"),
        )
        .await
        .is_none(),
        "an unconfirmed switch writes no chip"
    );

    let same = put_model(
        &state,
        &slug,
        &json!({ "instanceId": "claudeAgent", "model": "claude-sonnet-4-6" }),
    )
    .await
    .unwrap();
    assert_eq!(same["ok"], true);
    let bot = state.bots.get(&slug).await.unwrap().unwrap();
    assert_eq!(bot["model"]["model"], "claude-sonnet-4-6");
    assert_eq!(bot["model"]["instanceId"], "claudeAgent");
    let pushes = mock.model_pushes();
    assert_eq!(pushes.len(), 1);
    assert_eq!(pushes[0].project_id.as_deref(), Some("project-ada"));
    assert_eq!(pushes[0].thread_id.as_deref(), Some("thread-ada"));
    assert_eq!(pushes[0].selection["instanceId"], "claudeAgent");

    let held_again = put_model(&state, &slug, &switch).await.unwrap_err();
    assert_eq!(held_again.status, StatusCode::CONFLICT);
    assert_eq!(mock.model_pushes().len(), 1);
    assert_eq!(
        state.bots.get(&slug).await.unwrap().unwrap()["model"]["model"],
        "claude-sonnet-4-6"
    );

    let mut confirmed = switch;
    confirmed["confirmSwitch"] = json!(true);
    let applied = put_model(&state, &slug, &confirmed).await.unwrap();
    assert_eq!(applied["ok"], true);
    let bot = state.bots.get(&slug).await.unwrap().unwrap();
    assert_eq!(bot["model"]["instanceId"], "codex");
    assert_eq!(bot["model"]["model"], "gpt-5.4");
    assert_eq!(bot["runtimeMode"], "full-access");
    let pushes = mock.model_pushes();
    assert_eq!(pushes.len(), 2);
    assert_eq!(pushes[1].project_id.as_deref(), Some("project-ada"));
    assert_eq!(pushes[1].thread_id.as_deref(), Some("thread-ada"));
    assert_eq!(pushes[1].selection, bot["model"]);
    assert!(
        pushes
            .iter()
            .filter(|push| push.selection["instanceId"] == "codex")
            .count()
            == 1
    );
    let note = storage::read_json(
        &state
            .cfg
            .bots_root
            .join(".botapp")
            .join("provider-switches.json"),
    )
    .await
    .unwrap();
    assert_eq!(note[&slug]["name"], "codex");
    let view = chat_view(&state, &slug, None, None, None).await.unwrap();
    let chip = view["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["kind"] == "provider-switch")
        .unwrap();
    assert_eq!(chip["title"], "codex");
    assert_eq!(chip["text"], "Switched to");
}

#[tokio::test]
async fn a_bot_without_its_own_model_still_confirms_before_leaving_that_provider() {
    let dir = tempfile::tempdir().unwrap();
    let mock = Arc::new(MockHarness::new());
    let state = settings_state_with(dir.path(), harness(&mock));
    storage::write_json(
        &state.settings_path(),
        &json!({
            "defaultModel": { "instanceId": "codex", "model": "gpt-5.4", "options": [] }
        }),
    )
    .await
    .unwrap();
    let slug = ada(&state, Value::Null).await;

    let leave = json!({ "instanceId": "claudeAgent", "model": "claude-haiku-5-5" });
    let held = put_model(&state, &slug, &leave).await.unwrap_err();
    assert_eq!(held.status, StatusCode::CONFLICT);
    assert_eq!(held.message, CONFIRM_SWITCH);
    assert!(state.bots.get(&slug).await.unwrap().unwrap()["model"].is_null());
    assert!(mock.model_pushes().is_empty());

    let stayed = put_model(
        &state,
        &slug,
        &json!({ "instanceId": "codex", "model": "gpt-5.4-mini" }),
    )
    .await
    .unwrap();
    assert_eq!(stayed["ok"], true);
    assert_eq!(
        state.bots.get(&slug).await.unwrap().unwrap()["model"]["model"],
        "gpt-5.4-mini"
    );
    assert_eq!(mock.model_pushes().len(), 1);
    assert_eq!(mock.model_pushes()[0].selection["instanceId"], "codex");

    assert_eq!(
        put_model(&state, &slug, &leave).await.unwrap_err().status,
        StatusCode::CONFLICT
    );
    assert_eq!(mock.model_pushes().len(), 1);

    let mut confirmed = leave;
    confirmed["confirmSwitch"] = json!(true);
    put_model(&state, &slug, &confirmed).await.unwrap();
    let bot = state.bots.get(&slug).await.unwrap().unwrap();
    assert_eq!(bot["model"]["instanceId"], "claudeAgent");
    assert_eq!(bot["model"]["model"], "claude-haiku-5-5");
    let pushes = mock.model_pushes();
    assert_eq!(pushes.len(), 2);
    assert_eq!(pushes[1].selection["instanceId"], "claudeAgent");
    assert_eq!(pushes[1].thread_id.as_deref(), Some("thread-ada"));
}
