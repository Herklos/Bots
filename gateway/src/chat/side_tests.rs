use crate::api::settings_state_with;
use crate::api::{chat_view, list_side, merge_side, open_side, send_chat};
use crate::chat::side::{BOTH_LANES, NEED_MESSAGE, NOTHING_TO_MERGE, NOT_A_SIDE};
use crate::harness::adapters::mock::MockHarness;
use crate::harness::{ForkPoint, Harness, SideChatRef};
use crate::platform::error::ApiError;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

fn headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        "application/json".parse().unwrap(),
    );
    headers
}

fn harness(mock: &std::sync::Arc<MockHarness>) -> Option<std::sync::Arc<dyn Harness>> {
    Some(std::sync::Arc::clone(mock) as std::sync::Arc<dyn Harness>)
}

async fn ada(state: &crate::AppState) -> String {
    let created = state.bots.create(&json!({ "name": "Ada" })).await.unwrap();
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

fn user_message() -> Value {
    json!({
        "turnItems": [{
            "type": "user_message",
            "text": "hi",
            "createdBy": "user",
            "creationSource": "web",
        }]
    })
}

async fn post(
    state: &std::sync::Arc<crate::AppState>,
    slug: &str,
    body: Value,
) -> Result<Value, ApiError> {
    open_side(
        State(state.clone()),
        Path(slug.to_string()),
        headers(),
        Bytes::from(body.to_string()),
    )
    .await
    .map(|Json(value)| value)
}

#[tokio::test]
async fn a_fork_is_a_side_chat_and_a_later_send_stays_on_it() {
    let dir = tempfile::tempdir().unwrap();
    let mock = std::sync::Arc::new(MockHarness::new());
    mock.arm_forks();
    mock.set_projection("thread-ada", user_message());
    let state = settings_state_with(dir.path(), harness(&mock));
    let slug = ada(&state).await;
    let before = state.bots.get(&slug).await.unwrap().unwrap();

    let opened = post(&state, &slug, json!({ "text": "what time is it" }))
        .await
        .unwrap();
    let id = opened["threadId"].as_str().unwrap().to_string();
    assert_ne!(id, "thread-ada");
    assert!(!opened["messageId"].as_str().unwrap().is_empty());
    let forks = mock.fork_requests();
    assert_eq!(forks.len(), 1);
    assert!(forks[0].side_chat);
    assert_eq!(forks[0].point, ForkPoint::LatestStable);
    assert_eq!(forks[0].source_thread_id, "thread-ada");
    assert_eq!(forks[0].target_thread_id, id);
    assert_eq!(forks[0].title, "Side chat");
    assert_eq!(
        mock.sent_threads(),
        vec![(id.clone(), "what time is it".into())]
    );
    assert_eq!(mock.launches(), 0);

    let sent = send_chat(
        State(state.clone()),
        Path(slug.clone()),
        headers(),
        Bytes::from(json!({ "text": "still here", "side": id }).to_string()),
    )
    .await
    .unwrap();
    assert_eq!(sent.0["ok"], true);
    assert_eq!(
        mock.sent_threads(),
        vec![
            (id.clone(), "what time is it".into()),
            (id.clone(), "still here".into()),
        ]
    );
    let after = state.bots.get(&slug).await.unwrap().unwrap();
    assert_eq!(after["mainThreadId"], "thread-ada");
    assert_eq!(after["mainThreadId"], before["mainThreadId"]);
    assert!(after.get("sideThreadId").is_none());
    assert!(!after.to_string().contains(&id));
}

#[tokio::test]
async fn a_failed_first_message_deletes_the_unused_side_chat() {
    let dir = tempfile::tempdir().unwrap();
    let mock = std::sync::Arc::new(MockHarness::new());
    mock.arm_forks();
    mock.set_projection("thread-ada", user_message());
    mock.fail_next_send();
    let state = settings_state_with(dir.path(), harness(&mock));
    let slug = ada(&state).await;
    let err = post(&state, &slug, json!({ "text": "hello" }))
        .await
        .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_GATEWAY);
    let forks = mock.fork_requests();
    assert_eq!(forks.len(), 1);
    assert_eq!(
        mock.deleted_threads(),
        vec![forks[0].target_thread_id.clone()]
    );
    assert!(mock.sent_threads().is_empty());
    assert_eq!(mock.launches(), 0);
}

#[tokio::test]
async fn a_brief_alone_does_not_open_a_side_chat() {
    let dir = tempfile::tempdir().unwrap();
    let mock = std::sync::Arc::new(MockHarness::new());
    mock.arm_forks();
    mock.set_projection(
        "thread-ada",
        json!({
            "turnItems": [{
                "type": "user_message",
                "text": "You are Ada.",
                "createdBy": "agent",
                "creationSource": "server",
            }]
        }),
    );
    let state = settings_state_with(dir.path(), harness(&mock));
    let slug = ada(&state).await;
    let err = post(&state, &slug, json!({ "text": "" }))
        .await
        .unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert_eq!(err.message, NEED_MESSAGE);
    assert!(mock.fork_requests().is_empty());
    assert_eq!(mock.launches(), 0);
}

#[tokio::test]
async fn an_unknown_side_is_not_the_parent_thread() {
    let dir = tempfile::tempdir().unwrap();
    let mock = std::sync::Arc::new(MockHarness::new());
    mock.arm_forks();
    mock.set_projection(
        "thread-ada",
        json!({
            "turnItems": [{ "type": "user_message", "text": "only on the parent", "createdBy": "user" }],
            "runs": [{ "id": "parent-run", "status": "completed" }],
        }),
    );
    mock.set_side_chats(
        "thread-ada",
        vec![SideChatRef {
            id: "side-1".into(),
            title: "Side chat".into(),
        }],
    );
    mock.set_projection(
        "side-1",
        json!({
            "turnItems": [{ "type": "assistant_message", "text": "only on the side" }],
        }),
    );
    let state = settings_state_with(dir.path(), harness(&mock));
    let slug = ada(&state).await;

    let unknown = chat_view(&state, &slug, None, Some("nope"), None)
        .await
        .unwrap_err();
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST);
    assert_eq!(unknown.message, NOT_A_SIDE);

    let as_child = chat_view(&state, &slug, Some("side-1"), None, None)
        .await
        .unwrap_err();
    assert_eq!(as_child.status, StatusCode::BAD_REQUEST);
    assert_eq!(as_child.message, "That thread stays on the other machine.");

    let both = chat_view(&state, &slug, Some("side-1"), Some("side-1"), None)
        .await
        .unwrap_err();
    assert_eq!(both.status, StatusCode::BAD_REQUEST);
    assert_eq!(both.message, BOTH_LANES);

    let view = chat_view(&state, &slug, None, Some("side-1"), None)
        .await
        .unwrap();
    assert_eq!(view["threadId"], "side-1");
    assert!(view["messages"][0]["text"]
        .as_str()
        .unwrap()
        .contains("only on the side"));
    assert!(!view.to_string().contains("only on the parent"));
}

#[tokio::test]
async fn merge_without_a_run_does_not_call_merge_back() {
    let dir = tempfile::tempdir().unwrap();
    let mock = std::sync::Arc::new(MockHarness::new());
    mock.arm_forks();
    mock.set_side_chats(
        "thread-ada",
        vec![SideChatRef {
            id: "side-1".into(),
            title: "Side chat".into(),
        }],
    );
    mock.set_projection("side-1", json!({ "turnItems": [], "runs": [] }));
    let state = settings_state_with(dir.path(), harness(&mock));
    let slug = ada(&state).await;
    let err = merge_side(
        State(state),
        Path(slug),
        headers(),
        Bytes::from(json!({ "threadId": "side-1" }).to_string()),
    )
    .await
    .unwrap_err();
    assert_eq!(err.status, StatusCode::CONFLICT);
    assert_eq!(err.message, NOTHING_TO_MERGE);
    assert!(mock.merge_requests().is_empty());
}

#[tokio::test]
async fn the_list_is_newest_last_and_empty_before_a_thread_exists() {
    let dir = tempfile::tempdir().unwrap();
    let mock = std::sync::Arc::new(MockHarness::new());
    mock.arm_forks();
    mock.set_side_chats(
        "thread-ada",
        vec![
            SideChatRef {
                id: "older".into(),
                title: "Older".into(),
            },
            SideChatRef {
                id: "newer".into(),
                title: "Newer".into(),
            },
        ],
    );
    let state = settings_state_with(dir.path(), harness(&mock));
    let created = state.bots.create(&json!({ "name": "Bea" })).await.unwrap();
    let bare = created["slug"].as_str().unwrap();
    let empty = list_side(State(state.clone()), Path(bare.to_string()))
        .await
        .unwrap();
    assert_eq!(empty.0["chats"], json!([]));
    let no_thread = post(&state, bare, json!({ "text": "" })).await.unwrap_err();
    assert_eq!(no_thread.status, StatusCode::CONFLICT);
    assert_eq!(no_thread.message, "This Bot has no conversation yet");
    assert!(mock.fork_requests().is_empty());

    let slug = ada(&state).await;
    let listed = list_side(State(state.clone()), Path(slug)).await.unwrap();
    assert_eq!(listed.0["chats"][0]["id"], "older");
    assert_eq!(listed.0["chats"][1]["id"], "newer");
    assert_eq!(listed.0["chats"][1]["title"], "Newer");

    let missing = list_side(State(state), Path("missing".into()))
        .await
        .unwrap_err();
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}
