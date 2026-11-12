//! HTTP routes for the in-memory Anytype stand-in.

use super::search::{reject_search_body, search};
use super::state::{page, MockState, ReplayHit, Reply};
use super::Shared;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct PageQuery {
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct DeleteQuery {
    permanent: Option<bool>,
}

pub(super) fn router(state: Shared) -> Router {
    Router::new()
        .route("/v2/spaces/{space}/objects", post(create_object))
        .route(
            "/v2/spaces/{space}/objects/{id}",
            get(get_object).patch(patch_object).delete(delete_object),
        )
        .route(
            "/v2/spaces/{space}/objects/{id}/restore",
            post(restore_object),
        )
        .route("/v2/spaces/{space}/search", post(search_space))
        .route(
            "/v2/spaces/{space}/types",
            get(list_types).post(create_type),
        )
        .route(
            "/v2/spaces/{space}/properties",
            get(list_properties).post(create_property),
        )
        .route("/v2/spaces/{space}/files", post(upload_file))
        .route(
            "/v2/spaces/{space}/files/{id}/content",
            get(file_content).head(file_head),
        )
        .with_state(state)
}

async fn create_object(
    State(state): State<Shared>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    let parsed = parse_json(&body);
    mutation(&state, &headers, "POST", "/objects", &body, move |mock| {
        mock.create_object(&parsed)
    })
    .await
}

async fn get_object(
    State(state): State<Shared>,
    Path((_, id)): Path<(String, String)>,
) -> axum::response::Response {
    if let Some(response) = prelude(&state).await {
        return response;
    }
    let mock = state.lock().expect("mock");
    match mock.document(&id) {
        Some(reply) => respond(reply),
        None => respond(missing_reply()),
    }
}

async fn patch_object(
    State(state): State<Shared>,
    Path((_, id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    let if_match = header_str(&headers, header::IF_MATCH);
    let parsed = parse_json(&body);
    let path = format!("/objects/{id}");
    mutation(&state, &headers, "PATCH", &path, &body, move |mock| {
        mock.patch(&id, if_match.as_deref(), &parsed)
    })
    .await
}

async fn delete_object(
    State(state): State<Shared>,
    Path((space, id)): Path<(String, String)>,
    Query(query): Query<DeleteQuery>,
    headers: HeaderMap,
) -> axum::response::Response {
    let permanent = query.permanent.unwrap_or(false);
    let path = if permanent {
        format!("/objects/{id}?permanent=true")
    } else {
        format!("/objects/{id}")
    };
    let empty = Bytes::new();
    mutation(&state, &headers, "DELETE", &path, &empty, move |mock| {
        if permanent {
            mock.delete_permanent(&id)
        } else {
            mock.archive(&space, &id)
        }
    })
    .await
}

async fn restore_object(
    State(state): State<Shared>,
    Path((_, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> axum::response::Response {
    let empty = Bytes::new();
    let path = format!("/objects/{id}/restore");
    mutation(&state, &headers, "POST", &path, &empty, move |mock| {
        mock.restore(&id)
    })
    .await
}

async fn search_space(
    State(state): State<Shared>,
    Query(page_query): Query<PageQuery>,
    body: Bytes,
) -> axum::response::Response {
    if let Some(response) = prelude(&state).await {
        return response;
    }
    let query = parse_json(&body);
    if let Some(error) = reject_search_body(&query) {
        return respond(Reply {
            status: 400,
            body: error,
            etag: None,
        });
    }
    let mock = state.lock().expect("mock");
    let objects: Vec<_> = mock.objects.values().collect();
    let rows = search(&objects, &query);
    let (offset, limit) = page_bounds(&page_query);
    respond(Reply {
        status: 200,
        body: page(&rows, offset, limit),
        etag: None,
    })
}

async fn list_types(
    State(state): State<Shared>,
    Query(page_query): Query<PageQuery>,
) -> axum::response::Response {
    list_rows(&state, page_query, |mock| mock.types.clone()).await
}

async fn list_properties(
    State(state): State<Shared>,
    Query(page_query): Query<PageQuery>,
) -> axum::response::Response {
    list_rows(&state, page_query, |mock| mock.properties.clone()).await
}

async fn list_rows(
    state: &Shared,
    page_query: PageQuery,
    rows: impl FnOnce(&MockState) -> Vec<Value>,
) -> axum::response::Response {
    if let Some(response) = prelude(state).await {
        return response;
    }
    let mock = state.lock().expect("mock");
    let (offset, limit) = page_bounds(&page_query);
    respond(Reply {
        status: 200,
        body: page(&rows(&mock), offset, limit),
        etag: None,
    })
}

async fn create_type(
    State(state): State<Shared>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    let parsed = parse_json(&body);
    mutation(&state, &headers, "POST", "/types", &body, move |mock| {
        mock.add_type(&parsed)
    })
    .await
}

async fn create_property(
    State(state): State<Shared>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    let parsed = parse_json(&body);
    mutation(
        &state,
        &headers,
        "POST",
        "/properties",
        &body,
        move |mock| mock.add_property(&parsed),
    )
    .await
}

async fn upload_file(
    State(state): State<Shared>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    let content_type = header_str(&headers, header::CONTENT_TYPE).unwrap_or_default();
    let (name, mime, bytes) = if content_type.starts_with("multipart/") {
        file_part(&body)
    } else {
        (
            "file".to_string(),
            "application/octet-stream".to_string(),
            Vec::new(),
        )
    };
    mutation(&state, &headers, "POST", "/files", &body, move |mock| {
        mock.add_file(name, mime, bytes)
    })
    .await
}

async fn file_content(
    State(state): State<Shared>,
    Path((_, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> axum::response::Response {
    file_bytes(&state, &id, headers.get(header::RANGE), false).await
}

async fn file_head(
    State(state): State<Shared>,
    Path((_, id)): Path<(String, String)>,
) -> axum::response::Response {
    file_bytes(&state, &id, None, true).await
}

async fn file_bytes(
    state: &Shared,
    id: &str,
    range: Option<&HeaderValue>,
    head: bool,
) -> axum::response::Response {
    if let Some(response) = prelude(state).await {
        return response;
    }
    let mock = state.lock().expect("mock");
    let Some(file) = mock.files.get(id) else {
        return respond(missing_reply());
    };
    let etag = file.sha.clone();
    let bytes = if head { Vec::new() } else { file.bytes.clone() };
    let full = file.bytes.clone();
    drop(mock);
    if let Some(spec) = range.and_then(|value| value.to_str().ok()) {
        let Some((start, end)) = slice_range(spec, full.len()) else {
            return respond(Reply {
                status: 400,
                body: json!({"message": "The range is not valid."}),
                etag: None,
            });
        };
        return bytes_response(
            StatusCode::PARTIAL_CONTENT,
            &etag,
            full[start..=end].to_vec(),
        );
    }
    bytes_response(StatusCode::OK, &etag, bytes)
}

async fn mutation(
    state: &Shared,
    headers: &HeaderMap,
    method: &str,
    path: &str,
    body: &Bytes,
    apply: impl FnOnce(&mut MockState) -> Reply,
) -> axum::response::Response {
    if let Some(response) = prelude(state).await {
        return response;
    }
    let key = header_str(headers, "Idempotency-Key");
    let fingerprint = format!("{method} {path} {}", hex::encode(body));
    let mut mock = state.lock().expect("mock");
    if let Some(response) = replay(&mock, key.as_deref(), &fingerprint) {
        return response;
    }
    let reply = apply(&mut mock);
    if let Some(key) = key.as_deref() {
        mock.store_replay(key, fingerprint, reply.status, reply.body.clone());
    }
    respond(reply)
}

async fn prelude(state: &Shared) -> Option<axum::response::Response> {
    let (delay, not_granted, malformed, status_override, rate) = {
        let mut mock = state.lock().expect("mock");
        let rate = if mock.rate_limits_left > 0 {
            mock.rate_limits_left -= 1;
            Some(mock.retry_after_secs)
        } else {
            None
        };
        (
            mock.delay,
            mock.not_granted,
            mock.malformed,
            mock.status_override,
            rate,
        )
    };
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
    if not_granted {
        return Some(respond(Reply {
            status: 403,
            body: json!({"code": "forbidden", "message": "The key cannot use this space."}),
            etag: None,
        }));
    }
    if malformed {
        return Some(
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                "not-json",
            )
                .into_response(),
        );
    }
    if let Some(status) = status_override {
        return Some(respond(Reply {
            status,
            body: json!({"code": "gone", "message": "missing"}),
            etag: None,
        }));
    }
    if let Some(retry_after) = rate {
        let mut response = respond(Reply {
            status: 429,
            body: json!({
                "object": "error",
                "status": 429,
                "code": "rate_limit_exceeded",
                "message": "You have reached maximum request limit.",
            }),
            etag: None,
        });
        if retry_after > 0 {
            if let Ok(value) = HeaderValue::from_str(&retry_after.to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
        }
        return Some(response);
    }
    None
}

fn replay(
    mock: &MockState,
    key: Option<&str>,
    fingerprint: &str,
) -> Option<axum::response::Response> {
    let key = key?;
    match mock.replay(key, fingerprint) {
        ReplayHit::Miss => None,
        ReplayHit::Hit(status, body) => Some(respond(Reply {
            status,
            etag: body["etag"].as_str().map(str::to_string),
            body,
        })),
        ReplayHit::Conflict => Some(respond(Reply {
            status: 409,
            body: json!({
                "code": "idempotency_conflict",
                "message": "This idempotency key was used for a different request.",
            }),
            etag: None,
        })),
    }
}

fn respond(reply: Reply) -> axum::response::Response {
    let status = StatusCode::from_u16(reply.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut response = (status, Json(reply.body)).into_response();
    if let Some(etag) = reply.etag {
        if let Ok(value) = HeaderValue::from_str(&etag) {
            response.headers_mut().insert(header::ETAG, value);
        }
    }
    response
}

fn missing_reply() -> Reply {
    Reply {
        status: 404,
        body: json!({"code": "not_found", "message": "missing"}),
        etag: None,
    }
}

fn page_bounds(query: &PageQuery) -> (usize, usize) {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(25).clamp(1, 1000);
    (offset, limit)
}

fn parse_json(body: &Bytes) -> Value {
    serde_json::from_slice(body).unwrap_or_else(|_| json!({}))
}

fn header_str(headers: &HeaderMap, name: impl header::AsHeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn slice_range(spec: &str, len: usize) -> Option<(usize, usize)> {
    let rest = spec.trim().strip_prefix("bytes=")?;
    let (start, end) = rest.split_once('-')?;
    let start: usize = start.parse().ok()?;
    let end: usize = end.parse().ok()?;
    if start > end || end >= len {
        return None;
    }
    Some((start, end))
}

fn bytes_response(status: StatusCode, etag: &str, bytes: Vec<u8>) -> axum::response::Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if let Ok(value) = HeaderValue::from_str(etag) {
        headers.insert(header::ETAG, value);
    }
    (status, headers, bytes).into_response()
}

fn file_part(body: &[u8]) -> (String, String, Vec<u8>) {
    let header_end = body
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap_or(0);
    let headers = String::from_utf8_lossy(&body[..header_end]);
    let name = headers
        .split("filename=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or("upload")
        .to_string();
    let mime = part_content_type(&headers);
    let start = if header_end == 0 { 0 } else { header_end + 4 };
    let rest = &body[start..];
    let end = rest
        .windows(4)
        .position(|window| window == b"\r\n--")
        .unwrap_or(rest.len());
    (name, mime, rest[..end].to_vec())
}

fn part_content_type(headers: &str) -> String {
    for line in headers.split('\n') {
        let line = line.trim().trim_end_matches('\r');
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-type") {
            let mime = value.trim();
            if !mime.is_empty() {
                return mime.to_string();
            }
        }
    }
    "application/octet-stream".to_string()
}
