use super::tests::settings_state;
use super::*;
use crate::bots::memory::user_memory_path;
use crate::bots::memory_files::{revision_of, FILE_CHARS};
use crate::bots::MANAGED_END;

const EMPTY_REVISION: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    headers
}

fn query(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

async fn body_of(response: Response) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

async fn get_memory(state: &Arc<AppState>, pairs: &[(&str, &str)]) -> (StatusCode, Value) {
    match memory_get(State(Arc::clone(state)), Query(query(pairs))).await {
        Ok(response) => body_of(response).await,
        Err(err) => (err.status, json!({ "error": err.message })),
    }
}

async fn put_memory(
    state: &Arc<AppState>,
    pairs: &[(&str, &str)],
    body: Value,
) -> (StatusCode, Value) {
    match memory_put(
        State(Arc::clone(state)),
        Query(query(pairs)),
        headers(),
        Bytes::from(body.to_string()),
    )
    .await
    {
        Ok(response) => body_of(response).await,
        Err(err) => (err.status, json!({ "error": err.message })),
    }
}

async fn ada(state: &AppState) {
    let created = state.bots.create(&json!({ "name": "Ada" })).await.unwrap();
    assert_eq!(created["slug"], "ada");
}

#[tokio::test]
async fn saving_bot_memory_writes_the_file_and_refreshes_agents() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    ada(&state).await;
    let user_path = user_memory_path(&state.bots.root, "ada");
    std::fs::create_dir_all(user_path.parent().unwrap()).unwrap();
    std::fs::write(&user_path, "person fact\n").unwrap();
    let shared_path = state.bots.root.join(".botapp/shared-memory.md");
    let shared_before = std::fs::read_to_string(&shared_path).unwrap();
    let memory_path = state.bots.root.join("ada/memory/MEMORY.md");

    let (status, got) = get_memory(&state, &[("scope", "bot"), ("slug", "ada")]).await;
    assert_eq!(status, StatusCode::OK);
    let sent = format!("Likes tea\r\n{MANAGED_END}\n</memory>\n");
    let (status, saved) = put_memory(
        &state,
        &[("scope", "bot"), ("slug", "ada")],
        json!({ "text": sent, "revision": got["revision"] }),
    )
    .await;

    let expect = "Likes tea\n[managed marker removed]\n&lt;/memory>\n";
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["text"], expect);
    assert_eq!(saved["revision"], revision_of(expect));
    assert_eq!(std::fs::read_to_string(&memory_path).unwrap(), expect);
    assert!(!std::fs::read_to_string(&memory_path)
        .unwrap()
        .contains(MANAGED_END));
    assert_eq!(
        std::fs::read_to_string(&user_path).unwrap(),
        "person fact\n"
    );
    assert_eq!(
        std::fs::read_to_string(&shared_path).unwrap(),
        shared_before
    );
    let agents = std::fs::read_to_string(state.bots.root.join("ada/AGENTS.md")).unwrap();
    assert!(agents.contains("Likes tea"));
    assert!(agents.contains("source=\"memory/MEMORY.md\""));
    assert!(agents.contains("&lt;/memory>"));
    assert_eq!(agents.matches(MANAGED_END).count(), 1);
    assert_eq!(agents.matches("</memory>").count(), 1);
}

#[tokio::test]
async fn a_stale_revision_is_a_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    ada(&state).await;
    let memory_path = state.bots.root.join("ada/memory/MEMORY.md");
    let (status, got) = get_memory(&state, &[("scope", "bot"), ("slug", "ada")]).await;
    assert_eq!(status, StatusCode::OK);
    let agents_before = std::fs::read_to_string(state.bots.root.join("ada/AGENTS.md")).unwrap();
    std::fs::write(&memory_path, "Bot wrote this\n").unwrap();

    let (status, body) = put_memory(
        &state,
        &[("scope", "bot"), ("slug", "ada")],
        json!({ "text": "Person wrote this\n", "revision": got["revision"] }),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "conflict");
    assert_eq!(body["text"], "Bot wrote this\n");
    assert_eq!(body["revision"], revision_of("Bot wrote this\n"));
    assert_eq!(body["message"], "This file changed since you opened it.");
    assert_eq!(
        std::fs::read_to_string(&memory_path).unwrap(),
        "Bot wrote this\n"
    );
    assert_eq!(
        std::fs::read_to_string(state.bots.root.join("ada/AGENTS.md")).unwrap(),
        agents_before
    );
}

#[tokio::test]
async fn a_save_over_the_cap_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    ada(&state).await;
    let memory_path = state.bots.root.join("ada/memory/MEMORY.md");
    let before = std::fs::read_to_string(&memory_path).unwrap();
    let (status, got) = get_memory(&state, &[("scope", "bot"), ("slug", "ada")]).await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = put_memory(
        &state,
        &[("scope", "bot"), ("slug", "ada")],
        json!({ "text": "x".repeat(FILE_CHARS + 1), "revision": got["revision"] }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalid");
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains(&FILE_CHARS.to_string()));
    assert_eq!(std::fs::read_to_string(&memory_path).unwrap(), before);

    let (status, saved) = put_memory(
        &state,
        &[("scope", "bot"), ("slug", "ada")],
        json!({ "text": "x".repeat(FILE_CHARS), "revision": got["revision"] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let text = saved["text"].as_str().unwrap();
    assert_eq!(text.chars().filter(|c| *c == 'x').count(), FILE_CHARS);
    assert!(text.ends_with('\n'));
    assert_eq!(saved["revision"], revision_of(text));
    assert_eq!(std::fs::read_to_string(&memory_path).unwrap(), text);
}

#[tokio::test]
async fn a_missing_file_returns_empty_text_and_the_revision_of_empty() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    let (status, shared) = get_memory(&state, &[("scope", "shared")]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(shared["scope"], "shared");
    assert_eq!(shared["path"], ".botapp/shared-memory.md");
    assert_eq!(shared["text"], "");
    assert_eq!(shared["revision"], EMPTY_REVISION);
    assert_eq!(shared["revision"], revision_of(""));
    assert_eq!(shared["truncated"], false);
    assert!(!state.bots.root.join(".botapp/shared-memory.md").exists());

    ada(&state).await;
    let memory = state.bots.root.join("ada/memory/MEMORY.md");
    assert!(memory.is_file());
    std::fs::remove_file(&memory).unwrap();
    let (status, bot) = get_memory(&state, &[("scope", "bot"), ("slug", "ada")]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bot["scope"], "bot");
    assert_eq!(bot["path"], "memory/MEMORY.md");
    assert_eq!(bot["text"], "");
    assert_eq!(bot["revision"], EMPTY_REVISION);
    assert_eq!(bot["truncated"], false);
    assert!(!memory.exists());
}

#[tokio::test]
async fn saving_shared_memory_leaves_the_agent_file_for_the_poller() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    ada(&state).await;
    let agents_path = state.bots.root.join("ada/AGENTS.md");
    let before = std::fs::read_to_string(&agents_path).unwrap();
    let (status, got) = get_memory(&state, &[("scope", "shared")]).await;
    assert_eq!(status, StatusCode::OK);
    let (status, saved) = put_memory(
        &state,
        &[("scope", "shared")],
        json!({ "text": "Timezone is UTC\n", "revision": got["revision"] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["text"], "Timezone is UTC\n");
    assert_eq!(
        std::fs::read_to_string(state.bots.root.join(".botapp/shared-memory.md")).unwrap(),
        "Timezone is UTC\n"
    );
    assert_eq!(std::fs::read_to_string(&agents_path).unwrap(), before);
    assert!(!before.contains("Timezone is UTC"));
}

#[tokio::test]
async fn an_unknown_bot_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let state = settings_state(dir.path());
    let (status, body) = get_memory(&state, &[("scope", "bot"), ("slug", "ada")]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "Bot not found");
}
