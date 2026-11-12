//! Public webhook ingress. T3 checks the token, the signature, and the filters.
//! This route only forwards the raw body and stays outside sign-in.
//!
//! The hook needs the URL token and nothing else. Do not send the T3 bearer.
//! Do not reuse `proxy_http`: that helper adds the bearer and keeps four headers.

use super::*;
use crate::network::ratelimit::client_ip;
use std::time::Duration;

pub(crate) const BODY_LIMIT: usize = 1 << 20;

const SEGMENT: usize = 128;

pub(crate) async fn hook_route(
    State(s): S,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let started = Instant::now();
    let bytes = body.len();
    if !ingress_enabled() {
        return finish(StatusCode::NOT_FOUND, bytes, started);
    }
    if !hook_method(&method) {
        return finish(StatusCode::NOT_FOUND, bytes, started);
    }
    let ip = client_ip(&headers, peer.ip());
    if !s.hook_limit.allow(&ip) {
        return finish(StatusCode::TOO_MANY_REQUESTS, bytes, started);
    }
    let Some((task, token)) = hook_segments(uri.path()) else {
        return finish(StatusCode::NOT_FOUND, bytes, started);
    };
    let url = upstream(&s.cfg.t3_url, task, token, uri.query());
    if url::Url::parse(&url).is_err() {
        return finish(StatusCode::NOT_FOUND, bytes, started);
    }
    match hook_proxy(&method, &url, &headers, body).await {
        Ok(status) => finish(status, bytes, started),
        Err(()) => finish(StatusCode::BAD_GATEWAY, bytes, started),
    }
}

/// `HBOTS_WEBHOOKS=off` closes the route without a rebuild.
pub(crate) fn ingress_enabled() -> bool {
    std::env::var("HBOTS_WEBHOOKS")
        .ok()
        .as_deref()
        .map(str::trim)
        != Some("off")
}

fn hook_method(method: &Method) -> bool {
    matches!(*method, Method::POST | Method::PUT | Method::PATCH)
}

/// Both segments match `^[A-Za-z0-9:_\-.%]{1,128}$`. A decoded `.` / `..` or a
/// slash is refused so the upstream path cannot climb out of `/api/hooks/`.
fn hook_segments(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix("/hooks/")?;
    let (task, token) = rest.split_once('/')?;
    if token.contains('/') || !segment_ok(task) || !segment_ok(token) {
        return None;
    }
    Some((task, token))
}

fn segment_ok(segment: &str) -> bool {
    let len = segment.len();
    if !(1..=SEGMENT).contains(&len) {
        return false;
    }
    let allowed = segment.bytes().all(|byte| {
        matches!(
            byte,
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b':' | b'_' | b'-' | b'.' | b'%'
        )
    });
    allowed && !climbs(segment)
}

fn climbs(segment: &str) -> bool {
    let decoded = percent_decode(segment);
    decoded == "."
        || decoded == ".."
        || decoded.contains('/')
        || decoded.contains('\\')
        || decoded.contains("..")
}

fn percent_decode(segment: &str) -> String {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(value) = u8::from_str_radix(hex, 16) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn upstream(base: &str, task: &str, token: &str, query: Option<&str>) -> String {
    let base = base.trim_end_matches('/');
    match query {
        Some(query) if !query.is_empty() => format!("{base}/api/hooks/{task}/{token}?{query}"),
        _ => format!("{base}/api/hooks/{task}/{token}"),
    }
}

/// Stream the bytes unchanged. A re-encoded JSON body fails `x-hub-signature-256`.
async fn hook_proxy(
    method: &Method,
    url: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<StatusCode, ()> {
    let mut request = http_client().request(method.clone(), url).body(body);
    for (name, value) in headers {
        if keep_header(name) {
            request = request.header(name.clone(), value.clone());
        }
    }
    // The reqwest error includes the token URL, so it is discarded.
    let response = request.send().await.map_err(|_error| ())?;
    let status = response.status();
    // Drain so the socket can be reused. The bytes are not logged or returned.
    let _ = response.bytes().await;
    Ok(status)
}

fn keep_header(name: &header::HeaderName) -> bool {
    let name = name.as_str();
    !matches!(
        name,
        "host" | "connection" | "content-length" | "transfer-encoding" | "authorization" | "cookie"
    ) && !name.starts_with("x-forwarded-")
        && !name.starts_with("cf-")
}

fn http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .no_proxy()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

/// Fixed body so T3's error text and task ids never reach the sender.
fn finish(status: StatusCode, bytes: usize, started: Instant) -> Response {
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    tracing::info!(status = status.as_u16(), bytes, elapsed_ms, "hook");
    let ok = status.is_success();
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        Json(json!({ "ok": ok })),
    )
        .into_response()
}

/// Held by hook tests so `HBOTS_WEBHOOKS=off` does not race another request.
#[cfg(test)]
pub(crate) async fn test_env_lock() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}
