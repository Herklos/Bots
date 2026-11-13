//! The create route's webhook branch and the local Test delivery.

use super::tests::settings_state_with;
use super::*;
use crate::harness::adapters::mock::MockHarness;
use axum::body::Body;

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
    (dir, state, mock)
}

fn json_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    headers
}

async fn create(state: &Arc<AppState>, slug: &str, body: Value) -> Result<Json<Value>, ApiError> {
    bot_routine_save(
        State(Arc::clone(state)),
        Path(slug.to_string()),
        json_headers(),
        Bytes::from(body.to_string()),
    )
    .await
}

struct Hit {
    body: Vec<u8>,
    headers: HeaderMap,
}

async fn fake_t3() -> (String, Arc<std::sync::Mutex<Vec<Hit>>>) {
    let hits = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = Arc::clone(&hits);
    let router = Router::new().fallback(move |req: Request| {
        let recorded = Arc::clone(&recorded);
        async move {
            let (parts, body) = req.into_parts();
            let bytes = axum::body::to_bytes(body, 2 << 20)
                .await
                .unwrap_or_default();
            recorded.lock().unwrap().push(Hit {
                body: bytes.to_vec(),
                headers: parts.headers,
            });
            Response::builder()
                .status(StatusCode::ACCEPTED)
                .header("x-t3-hook-outcome", "accepted")
                .body(Body::from(r#"{"ok":true}"#))
                .unwrap()
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{addr}"), hits)
}

#[tokio::test]
async fn a_webhook_create_returns_the_row_and_the_address_once() {
    let (_dir, state, _mock) = ready().await;
    let saved = create(
        &state,
        "ada",
        json!({
            "kind": "webhook",
            "title": "GitHub pings",
            "prompt": "A webhook arrived.",
            "preset": "github",
            "secret": "s3cret-write-only",
            "enable": true,
            "maxAgeMinutes": 120
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(saved["ok"], true);
    let row = &saved["task"];
    assert_eq!(row["kind"], "webhook");
    assert_eq!(row["title"], "GitHub pings");
    assert_eq!(row["enabled"], true);
    assert_eq!(row["hook"]["hasSecret"], true);
    assert!(row.get("url").is_none());
    let text = saved.to_string();
    assert!(!text.contains("s3cret-write-only"));
    let url = saved["hook"]["url"].as_str().unwrap();
    assert!(url.starts_with("https://hooks.example/hooks/"));
    let token = url.rsplit('/').next().unwrap();
    assert!(!token.is_empty());
    assert_eq!(saved["hook"]["source"], "publicBase");
    assert_eq!(saved["hook"]["stable"], true);

    // The polled list stays token-free.
    let listed = bot_routines(State(Arc::clone(&state)), Path("ada".into()))
        .await
        .unwrap()
        .0;
    assert!(!listed.to_string().contains(token));
    assert!(!listed.to_string().contains("https://hooks.example"));
}

#[tokio::test]
async fn the_schedule_kinds_still_create_without_an_address() {
    let (_dir, state, _mock) = ready().await;
    for (kind, label) in [("hour", "Every hour"), ("day", "Every day at 9:00 AM")] {
        let saved = create(&state, "ada", json!({ "kind": kind, "title": "Morning" }))
            .await
            .unwrap()
            .0;
        assert_eq!(saved["task"]["kind"], "schedule");
        assert_eq!(saved["task"]["schedule"], label);
        assert!(saved.get("hook").is_none());
    }
    let plain = create(&state, "ada", json!({ "kind": "once", "title": "Later" }))
        .await
        .unwrap()
        .0;
    assert_eq!(plain["task"]["paused"], true);
    let refused = create(
        &state,
        "ada",
        json!({ "kind": "other", "title": "No such kind" }),
    )
    .await
    .unwrap_err();
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
}

async fn test_call(
    state: &Arc<AppState>,
    slug: &str,
    id: &str,
    body: Value,
) -> Result<Json<Value>, ApiError> {
    bot_routine_test(
        State(Arc::clone(state)),
        Path((slug.to_string(), id.to_string())),
        json_headers(),
        Bytes::from(body.to_string()),
    )
    .await
}

#[tokio::test]
async fn the_test_delivery_signs_only_a_reentered_secret() {
    let (_dir, state, _mock) = ready().await;
    let saved = create(
        &state,
        "ada",
        json!({
            "kind": "webhook",
            "title": "Signed",
            "prompt": "A webhook arrived.",
            "preset": "github",
            "secret": "s3cret-used-once"
        }),
    )
    .await
    .unwrap()
    .0;
    let id = saved["task"]["id"].as_str().unwrap().to_string();
    let (t3, hits) = fake_t3().await;
    let mut state = state;
    Arc::get_mut(&mut state).unwrap().cfg.t3_url = t3;

    // Unsigned first: no signature header reaches T3.
    let cleared = test_call(&state, "ada", &id, json!({ "body": "{\"hello\":1}" }))
        .await
        .unwrap()
        .0;
    assert_eq!(cleared["ok"], true);
    assert_eq!(cleared["outcome"], "accepted");
    {
        let hits_now = hits.lock().unwrap();
        assert_eq!(hits_now.len(), 1);
        assert_eq!(hits_now[0].body, b"{\"hello\":1}".to_vec());
        assert!(hits_now[0].headers.get("x-hub-signature-256").is_none());
        assert!(hits_now[0].headers.get("authorization").is_none());
    }

    // Re-entered secret: the signature is computed over the raw bytes, and the
    // secret itself never appears in the request.
    let signed = test_call(
        &state,
        "ada",
        &id,
        json!({ "body": "{\"hello\":1}", "secret": "s3cret-used-once" }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(signed["ok"], true);
    let hits_after = hits.lock().unwrap();
    assert_eq!(hits_after.len(), 2);
    let header = hits_after[1]
        .headers
        .get("x-hub-signature-256")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(header.starts_with("sha256="));
    use hmac::Mac;
    let mut mac = <hmac::Hmac<sha2::Sha256>>::new_from_slice(b"s3cret-used-once").unwrap();
    mac.update(b"{\"hello\":1}");
    let digest = hex::encode(mac.finalize().into_bytes());
    assert_eq!(header, format!("sha256={digest}"));
    let all = format!("{:?}", hits_after[1].headers);
    assert!(!all.contains("s3cret-used-once"));
}

#[tokio::test]
async fn the_test_delivery_stays_on_this_bot_and_needs_a_signature_check() {
    let (_dir, state, _mock) = ready().await;
    let saved = create(
        &state,
        "ada",
        json!({
            "kind": "webhook",
            "title": "Unsigned",
            "prompt": "A webhook arrived.",
            "preset": "token"
        }),
    )
    .await
    .unwrap()
    .0;
    let id = saved["task"]["id"].as_str().unwrap().to_string();
    state.bots.create(&json!({ "name": "Bea" })).await.unwrap();
    state
        .bots
        .save("bea", &json!({ "mainThreadId": "bea-thread" }), false)
        .await
        .unwrap();
    let other = test_call(&state, "bea", &id, json!({ "body": "x" }))
        .await
        .unwrap_err();
    assert_eq!(other.status, StatusCode::NOT_FOUND);
    // A secret typed for a task with no signature check is refused, not ignored.
    let refused = test_call(
        &state,
        "ada",
        &id,
        json!({ "body": "x", "secret": "no-check" }),
    )
    .await
    .unwrap_err();
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert_eq!(refused.message, "This webhook has no signature check.");
    let missing = test_call(&state, "ada", &id, json!({ "other": 1 }))
        .await
        .unwrap_err();
    assert_eq!(missing.status, StatusCode::BAD_REQUEST);
    let gone = test_call(
        &state,
        "ada",
        "scheduled-task:missing",
        json!({ "body": "x" }),
    )
    .await
    .unwrap_err();
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
}
