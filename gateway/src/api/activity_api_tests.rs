use super::tests::settings_state_with;
use super::*;
use crate::harness::adapters::mock::MockHarness;
use crate::harness::Harness;

async fn bot(state: &AppState, name: &str, thread: &str) -> String {
    let created = state.bots.create(&json!({ "name": name })).await.unwrap();
    let slug = created["slug"].as_str().unwrap().to_string();
    state
        .bots
        .save(
            &slug,
            &json!({ "projectId": format!("project-{thread}"), "mainThreadId": thread }),
            false,
        )
        .await
        .unwrap();
    slug
}

#[tokio::test]
async fn activity_detail_is_this_threads_call_and_a_vault_call_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let mock = Arc::new(MockHarness::new());
    mock.set_projection(
        "thread-ada",
        json!({
            "turnItems": [
                {
                    "id": "c",
                    "type": "command_execution",
                    "status": "completed",
                    "ordinal": 1,
                    "command": "echo sk-abcdefghij123456",
                    "output": "sk-abcdefghij123456",
                },
                {
                    "id": "v",
                    "type": "dynamic_tool",
                    "status": "completed",
                    "ordinal": 2,
                    "toolName": "vault_fill_password",
                    "input": { "password": "vault-secret-value" },
                    "output": "vault-secret-value",
                }
            ]
        }),
    );
    let state = settings_state_with(dir.path(), Some(Arc::clone(&mock) as Arc<dyn Harness>));
    let ada = bot(&state, "Ada", "thread-ada").await;
    let bea = bot(&state, "Bea", "thread-bea").await;

    let found = read_activity(
        State(state.clone()),
        Path((ada.clone(), "c".into())),
        Query(HashMap::new()),
    )
    .await
    .unwrap()
    .0;
    assert!(found["output"].as_str().unwrap().contains("[redacted]"));
    assert!(!found.to_string().contains("sk-abcdefghij123456"));

    let vault = read_activity(
        State(state.clone()),
        Path((ada, "v".into())),
        Query(HashMap::new()),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(vault["input"], "");
    assert_eq!(vault["output"], "");
    assert!(!vault.to_string().contains("vault-secret-value"));

    let other = read_activity(State(state), Path((bea, "c".into())), Query(HashMap::new()))
        .await
        .unwrap_err();
    assert_eq!(other.status, StatusCode::NOT_FOUND);
}
