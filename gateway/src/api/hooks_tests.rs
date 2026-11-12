//! Ingress against a local axum stand-in for T3. The stand-in records the raw
//! request and answers with a body the gateway must not relay.

use super::hooks_api::{test_env_lock, BODY_LIMIT};
use super::tests::settings_state;
use super::*;
use crate::network::ratelimit::PER_IP_BURST;
use axum::body::Body;
use std::net::SocketAddr;
use tower::ServiceExt;

struct Hit {
    method: String,
    path: String,
    query: String,
    body: Vec<u8>,
    headers: HeaderMap,
}

fn decide(token: &str) -> (StatusCode, String) {
    match token {
        "good-token" | "TokSecret9f3aZZ" => {
            (StatusCode::ACCEPTED, format!("accepted leak {token}"))
        }
        "bad-token" => (StatusCode::NOT_FOUND, format!("not_found leak {token}")),
        "signed" => (
            StatusCode::UNAUTHORIZED,
            "rejected_signature task scheduled-task:abc".into(),
        ),
        _ => (StatusCode::NOT_FOUND, "not_found".into()),
    }
}

async fn fake_t3() -> (String, Arc<Mutex<Vec<Hit>>>) {
    let hits = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&hits);
    let router = Router::new().fallback(move |req: Request| {
        let recorded = Arc::clone(&recorded);
        async move {
            let (parts, body) = req.into_parts();
            let bytes = axum::body::to_bytes(body, 2 << 20)
                .await
                .unwrap_or_default();
            let path = parts.uri.path().to_string();
            let query = parts.uri.query().unwrap_or("").to_string();
            let token = path.rsplit('/').next().unwrap_or("");
            let (status, text) = decide(token);
            recorded.lock().unwrap().push(Hit {
                method: parts.method.to_string(),
                path,
                query,
                body: bytes.to_vec(),
                headers: parts.headers,
            });
            Response::builder()
                .status(status)
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-t3-hook-outcome", "accepted")
                .body(Body::from(text))
                .unwrap()
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://127.0.0.1:{port}"), hits)
}

fn gateway(dir: &std::path::Path, t3: &str) -> Arc<AppState> {
    let mut state = settings_state(dir);
    Arc::get_mut(&mut state).unwrap().cfg.t3_url = t3.to_string();
    state
}

async fn call(
    state: &Arc<AppState>,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Vec<u8>,
) -> (StatusCode, String, HeaderMap) {
    let mut builder = Request::builder().method(method).uri(path);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let mut request = builder.body(Body::from(body)).unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 9))));
    let response = app(Arc::clone(state)).oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap_or_default();
    (
        status,
        String::from_utf8_lossy(&bytes).into_owned(),
        headers,
    )
}

fn has_header(hit: &Hit, name: &str) -> bool {
    hit.headers
        .iter()
        .any(|(key, _)| key.as_str().eq_ignore_ascii_case(name))
}

#[tokio::test]
async fn the_raw_body_and_signature_are_forwarded_and_authorization_is_dropped() {
    let _guard = test_env_lock().await;
    let dir = tempfile::tempdir().unwrap();
    let (t3, hits) = fake_t3().await;
    let state = gateway(dir.path(), &t3);
    let body = b"{\"sig\": 2}\xff".to_vec();
    let (status, text, response_headers) = call(
        &state,
        "POST",
        "/hooks/scheduled-task%3Aabc/good-token?x=1",
        &[
            ("content-type", "application/json"),
            ("x-hub-signature-256", "sha256=abc"),
            ("authorization", "Bearer sender-secret"),
            ("cookie", "bot_session=secret"),
            ("x-forwarded-for", "198.51.100.1, 203.0.113.4"),
            ("cf-ray", "ray-1"),
            ("host", "evil.example"),
        ],
        body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(text, r#"{"ok":true}"#);
    assert!(response_headers.get("x-t3-hook-outcome").is_none());
    assert!(!text.contains("leak"));
    let hits = hits.lock().unwrap();
    let hit = &hits[0];
    assert_eq!(hit.method, "POST");
    assert_eq!(hit.path, "/api/hooks/scheduled-task%3Aabc/good-token");
    assert_eq!(hit.query, "x=1");
    assert_eq!(hit.body, body);
    assert_eq!(
        hit.headers.get("x-hub-signature-256").unwrap(),
        "sha256=abc"
    );
    assert_eq!(hit.headers.get("content-type").unwrap(), "application/json");
    assert!(!has_header(hit, "authorization"));
    assert!(!has_header(hit, "cookie"));
    assert!(!has_header(hit, "x-forwarded-for"));
    assert!(!has_header(hit, "cf-ray"));
    assert_ne!(
        hit.headers.get("host").and_then(|v| v.to_str().ok()),
        Some("evil.example")
    );
    let dumped = format!("{:?}", hit.headers);
    assert!(!dumped.contains("sender-secret"));
    assert!(!dumped.contains("fixture-only"));
}

#[tokio::test]
async fn put_is_forwarded_and_get_is_not() {
    let _guard = test_env_lock().await;
    let dir = tempfile::tempdir().unwrap();
    let (t3, hits) = fake_t3().await;
    let state = gateway(dir.path(), &t3);
    let (status, _, _) = call(
        &state,
        "PUT",
        "/hooks/task/good-token",
        &[],
        b"put".to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(hits.lock().unwrap()[0].method, "PUT");
    let before = hits.lock().unwrap().len();
    let (status, text, _) = call(&state, "GET", "/hooks/task/good-token", &[], Vec::new()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(text, r#"{"ok":false}"#);
    let (status, _, _) = call(&state, "DELETE", "/hooks/task/good-token", &[], Vec::new()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(hits.lock().unwrap().len(), before);
}

#[tokio::test]
async fn a_bad_token_and_a_bad_path_are_the_same_404() {
    let _guard = test_env_lock().await;
    let dir = tempfile::tempdir().unwrap();
    let (t3, hits) = fake_t3().await;
    let state = gateway(dir.path(), &t3);
    let bad_path = call(&state, "POST", "/hooks/@@@/good-token", &[], b"{}".to_vec()).await;
    let bad_token = call(&state, "POST", "/hooks/task/bad-token", &[], b"{}".to_vec()).await;
    assert_eq!(bad_path.0, StatusCode::NOT_FOUND);
    assert_eq!(bad_token.0, StatusCode::NOT_FOUND);
    assert_eq!(bad_path.1, bad_token.1);
    assert_eq!(bad_path.1, r#"{"ok":false}"#);
    assert!(!bad_token.1.contains("leak"));
    assert!(!bad_token.1.contains("task"));
    assert_eq!(hits.lock().unwrap().len(), 1);
    let (status, text, _) = call(&state, "POST", "/hooks/task/signed", &[], b"{}".to_vec()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(text, r#"{"ok":false}"#);
    assert!(!text.contains("rejected_signature"));
    assert!(!text.contains("scheduled-task"));
}

#[tokio::test]
async fn one_mebibyte_is_accepted_and_one_more_byte_is_413() {
    let _guard = test_env_lock().await;
    let dir = tempfile::tempdir().unwrap();
    let (t3, hits) = fake_t3().await;
    let state = gateway(dir.path(), &t3);
    let exact = vec![b'a'; BODY_LIMIT];
    let (status, _, _) = call(
        &state,
        "POST",
        "/hooks/task/good-token",
        &[("content-type", "application/octet-stream")],
        exact.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(hits.lock().unwrap()[0].body, exact);
    let over = vec![b'b'; BODY_LIMIT + 1];
    let (status, _, _) = call(&state, "POST", "/hooks/task/good-token", &[], over).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(hits.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn the_per_address_bucket_trips() {
    let _guard = test_env_lock().await;
    let dir = tempfile::tempdir().unwrap();
    let (t3, hits) = fake_t3().await;
    let state = gateway(dir.path(), &t3);
    let forwarded = [("x-forwarded-for", "203.0.113.8")];
    for _ in 0..PER_IP_BURST {
        let (status, _, _) = call(
            &state,
            "POST",
            "/hooks/task/good-token",
            &forwarded,
            b"{}".to_vec(),
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED);
    }
    let (status, text, _) = call(
        &state,
        "POST",
        "/hooks/task/good-token",
        &forwarded,
        b"{}".to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(text, r#"{"ok":false}"#);
    assert_eq!(hits.lock().unwrap().len(), PER_IP_BURST as usize);
    let (status, _, _) = call(
        &state,
        "POST",
        "/hooks/task/good-token",
        &[("x-forwarded-for", "203.0.113.9")],
        b"{}".to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn webhooks_off_is_404_and_does_not_call_upstream() {
    let _guard = test_env_lock().await;
    let dir = tempfile::tempdir().unwrap();
    let (t3, hits) = fake_t3().await;
    let state = gateway(dir.path(), &t3);
    std::env::set_var("HBOTS_WEBHOOKS", "off");
    let _off = ClearWebhooksEnv;
    let (status, text, _) = call(
        &state,
        "POST",
        "/hooks/task/good-token",
        &[],
        b"{}".to_vec(),
    )
    .await;
    drop(_off);
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(text, r#"{"ok":false}"#);
    assert!(hits.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn the_token_is_absent_from_tracing() {
    let _guard = test_env_lock().await;
    let dir = tempfile::tempdir().unwrap();
    let (t3, _) = fake_t3().await;
    let state = gateway(dir.path(), &t3);
    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .with_writer(BufWriter(Arc::clone(&buf)))
        .finish();
    let _default = tracing::subscriber::set_default(subscriber);
    let token = "TokSecret9f3aZZ";
    let (status, text, _) = call(
        &state,
        "POST",
        &format!("/hooks/task/{token}?n={token}"),
        &[("x-hub-signature-256", token)],
        b"raw".to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(!text.contains(token));
    drop(_default);
    let logged = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    assert!(logged.contains("status=202"), "{logged}");
    assert!(logged.contains("bytes=3"), "{logged}");
    assert!(!logged.contains(token), "{logged}");
}

struct ClearWebhooksEnv;

impl Drop for ClearWebhooksEnv {
    fn drop(&mut self) {
        std::env::remove_var("HBOTS_WEBHOOKS");
    }
}

#[derive(Clone)]
struct BufWriter(Arc<Mutex<Vec<u8>>>);

struct BufWrite(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for BufWrite {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for BufWriter {
    type Writer = BufWrite;
    fn make_writer(&'a self) -> Self::Writer {
        BufWrite(Arc::clone(&self.0))
    }
}
