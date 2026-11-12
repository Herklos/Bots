//! HTTP routes for the in-memory Anytype stand-in.

use super::search::search;
use super::state::{object_json, page, MockState, ReplayHit};
use super::Shared;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
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

pub(super) fn router(state: Shared) -> Router {
    Router::new()
        .route("/v2/spaces/{space}/objects", post(create_object))
        .route(
            "/v2/spaces/{space}/objects/{id}",
            get(get_object).patch(patch_object).delete(archive_object),
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
        .route("/v2/spaces/{space}/files/{id}", get(file_meta))
        .route("/v2/spaces/{space}/files/{id}/content", get(file_content))
        .with_state(state)
}

async fn create_object(
    State(state): State<Shared>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    if let Some(response) = prelude(&state).await {
        return response;
    }
    mutate(&state, &headers, "POST", "/objects", &body, 201, |mock| {
        mock.create_object(&parse_json(&body))
    })
}

async fn get_object(
    State(state): State<Shared>,
    Path((_, id)): Path<(String, String)>,
) -> axum::response::Response {
    if let Some(response) = prelude(&state).await {
        return response;
    }
    let mock = state.lock().expect("mock");
    match mock.objects.get(&id) {
        Some(object) => json_response(StatusCode::OK, object_json(object)),
        None => gone(),
    }
}

async fn patch_object(
    State(state): State<Shared>,
    Path((_, id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    if let Some(response) = prelude(&state).await {
        return response;
    }
    {
        let mock = state.lock().expect("mock");
        if let Some(response) = write_guard(&mock, &headers, mock.objects.get(&id)) {
            return response;
        }
    }
    mutate(
        &state,
        &headers,
        "PATCH",
        &format!("/objects/{id}"),
        &body,
        200,
        |mock| {
            mock.patch_object(&id, &parse_json(&body))
                .unwrap_or_else(|| json!({"code": "not_found"}))
        },
    )
}

async fn archive_object(
    State(state): State<Shared>,
    Path((_, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> axum::response::Response {
    if let Some(response) = prelude(&state).await {
        return response;
    }
    {
        let mock = state.lock().expect("mock");
        if let Some(response) = write_guard(&mock, &headers, mock.objects.get(&id)) {
            return response;
        }
    }
    let empty = Bytes::new();
    mutate(
        &state,
        &headers,
        "DELETE",
        &format!("/objects/{id}"),
        &empty,
        200,
        |mock| {
            let Some(object) = mock.objects.get_mut(&id) else {
                return json!({"code": "not_found"});
            };
            object.archived = true;
            object_json(object)
        },
    )
}

async fn search_space(
    State(state): State<Shared>,
    Query(page_query): Query<PageQuery>,
    body: Bytes,
) -> axum::response::Response {
    if let Some(response) = prelude(&state).await {
        return response;
    }
    let mock = state.lock().expect("mock");
    let query = parse_json(&body);
    let objects: Vec<_> = mock.objects.values().collect();
    let rows = search(&objects, &query);
    let (offset, limit) = page_bounds(&page_query);
    json_response(StatusCode::OK, page(&rows, offset, limit))
}

async fn list_types(
    State(state): State<Shared>,
    Query(page_query): Query<PageQuery>,
) -> axum::response::Response {
    if let Some(response) = prelude(&state).await {
        return response;
    }
    let mock = state.lock().expect("mock");
    let (offset, limit) = page_bounds(&page_query);
    json_response(StatusCode::OK, page(&mock.types, offset, limit))
}

async fn list_properties(
    State(state): State<Shared>,
    Query(page_query): Query<PageQuery>,
) -> axum::response::Response {
    if let Some(response) = prelude(&state).await {
        return response;
    }
    let mock = state.lock().expect("mock");
    let (offset, limit) = page_bounds(&page_query);
    json_response(StatusCode::OK, page(&mock.properties, offset, limit))
}

async fn create_type(
    State(state): State<Shared>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    if let Some(response) = prelude(&state).await {
        return response;
    }
    mutate(&state, &headers, "POST", "/types", &body, 201, |mock| {
        mock.add_type(&parse_json(&body))
    })
}

async fn create_property(
    State(state): State<Shared>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    if let Some(response) = prelude(&state).await {
        return response;
    }
    mutate(
        &state,
        &headers,
        "POST",
        "/properties",
        &body,
        201,
        |mock| mock.add_property(&parse_json(&body)),
    )
}

async fn upload_file(
    State(state): State<Shared>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    if let Some(response) = prelude(&state).await {
        return response;
    }
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let (name, mime, bytes) = if content_type.starts_with("multipart/") {
        let (name, bytes) = file_part(&body);
        (name, "application/octet-stream".into(), bytes)
    } else {
        let json = parse_json(&body);
        let name = json["name"].as_str().unwrap_or("file").to_string();
        let url = json["url"].as_str().unwrap_or("").to_string();
        (name, "text/uri-list".into(), url.into_bytes())
    };
    mutate(
        &state,
        &headers,
        "POST",
        "/files",
        &body,
        201,
        move |mock| mock.add_file(name, mime, bytes),
    )
}

async fn file_meta(
    State(state): State<Shared>,
    Path((_, id)): Path<(String, String)>,
) -> axum::response::Response {
    if let Some(response) = prelude(&state).await {
        return response;
    }
    let mock = state.lock().expect("mock");
    match mock.files.get(&id) {
        Some(file) => json_response(
            StatusCode::OK,
            json!({
                "id": file.id,
                "name": file.name,
                "mime_type": file.mime,
                "size": file.bytes.len(),
                "sha": file.sha,
            }),
        ),
        None => gone(),
    }
}

async fn file_content(
    State(state): State<Shared>,
    Path((_, id)): Path<(String, String)>,
) -> axum::response::Response {
    if let Some(response) = prelude(&state).await {
        return response;
    }
    let mock = state.lock().expect("mock");
    match mock.files.get(&id) {
        Some(file) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/octet-stream")],
            file.bytes.clone(),
        )
            .into_response(),
        None => gone(),
    }
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
        return Some(json_response(
            StatusCode::FORBIDDEN,
            json!({"code": "forbidden", "message": "The key cannot use this space."}),
        ));
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
        let code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST);
        return Some(json_response(
            code,
            json!({"code": "gone", "message": "missing"}),
        ));
    }
    if let Some(retry_after) = rate {
        return Some(
            (
                StatusCode::TOO_MANY_REQUESTS,
                [
                    (header::RETRY_AFTER, retry_after.to_string()),
                    (header::CONTENT_TYPE, "application/json".into()),
                ],
                Json(json!({"code": "rate_limit_exceeded", "message": "Rate limit exceeded"})),
            )
                .into_response(),
        );
    }
    None
}

fn write_guard(
    mock: &MockState,
    headers: &HeaderMap,
    object: Option<&super::state::Object>,
) -> Option<axum::response::Response> {
    let Some(object) = object else {
        return Some(gone());
    };
    if object.archived || mock.archived_writes {
        return Some(json_response(
            StatusCode::CONFLICT,
            json!({"code": "object_archived", "message": "The object is archived."}),
        ));
    }
    let presented = headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok());
    let stale = presented.is_some_and(|etag| etag != object.etag);
    if mock.etag_mismatch || stale {
        return Some(json_response(
            StatusCode::CONFLICT,
            json!({
                "code": "etag_mismatch",
                "message": "The object changed.",
                "etag": object.etag,
            }),
        ));
    }
    None
}

fn mutate(
    state: &Shared,
    headers: &HeaderMap,
    method: &str,
    path: &str,
    body: &Bytes,
    status: u16,
    apply: impl FnOnce(&mut MockState) -> Value,
) -> axum::response::Response {
    let key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let fingerprint = format!("{method} {path} {}", hex::encode(body));
    let mut mock = state.lock().expect("mock");
    if let Some(key) = key.as_deref() {
        match mock.replay(key, &fingerprint) {
            ReplayHit::Hit(status, body) => {
                return json_response(StatusCode::from_u16(status).unwrap_or(StatusCode::OK), body);
            }
            ReplayHit::Conflict => {
                return json_response(
                    StatusCode::CONFLICT,
                    json!({
                        "code": "idempotency_conflict",
                        "message": "This idempotency key was used for a different request.",
                    }),
                );
            }
            ReplayHit::Miss => {}
        }
    }
    let body = apply(&mut mock);
    let http = StatusCode::from_u16(status).unwrap_or(StatusCode::OK);
    if let Some(key) = key.as_deref() {
        mock.store_replay(key, fingerprint, status, body.clone());
    }
    json_response(http, body)
}

fn page_bounds(query: &PageQuery) -> (usize, usize) {
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(25).clamp(1, 1000);
    (offset, limit)
}

fn parse_json(body: &Bytes) -> Value {
    serde_json::from_slice(body).unwrap_or_else(|_| json!({}))
}

fn gone() -> axum::response::Response {
    json_response(
        StatusCode::NOT_FOUND,
        json!({"code": "not_found", "message": "missing"}),
    )
}

fn json_response(status: StatusCode, body: Value) -> axum::response::Response {
    (status, Json(body)).into_response()
}

fn file_part(body: &[u8]) -> (String, Vec<u8>) {
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
    let start = if header_end == 0 { 0 } else { header_end + 4 };
    let rest = &body[start..];
    let end = rest
        .windows(4)
        .position(|window| window == b"\r\n--")
        .unwrap_or(rest.len());
    (name, rest[..end].to_vec())
}
