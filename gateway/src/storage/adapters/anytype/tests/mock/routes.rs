//! HTTP routes for the in-memory Anytype stand-in.

use super::routes_files;
use super::routes_types;
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
            "/v2/spaces/{space}/search/stream",
            post(super::stream::search_stream),
        )
        .route(
            "/v2/spaces/{space}/types",
            get(list_types).post(create_type),
        )
        .route(
            "/v2/spaces/{space}/types/{type}",
            get(routes_types::get_type).patch(routes_types::patch_type),
        )
        .route(
            "/v2/spaces/{space}/properties",
            get(list_properties).post(create_property),
        )
        .route("/v2/spaces/{space}/files", post(routes_files::upload_file))
        .route(
            "/v2/spaces/{space}/files/{id}/content",
            get(routes_files::file_content).head(routes_files::file_head),
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
    let mut mock = state.lock().expect("mock");
    mock.gets += 1;
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
    if let Some(error) = mock.reject_unknown(&query) {
        return respond(Reply {
            status: 400,
            body: error,
            etag: None,
        });
    }
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

pub(super) async fn mutation(
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

pub(super) async fn prelude(state: &Shared) -> Option<axum::response::Response> {
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

pub(super) fn respond(reply: Reply) -> axum::response::Response {
    let status = StatusCode::from_u16(reply.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut response = (status, Json(reply.body)).into_response();
    if let Some(etag) = reply.etag {
        if let Ok(value) = HeaderValue::from_str(&etag) {
            response.headers_mut().insert(header::ETAG, value);
        }
    }
    response
}

pub(super) fn missing_reply() -> Reply {
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

pub(super) fn parse_json(body: &Bytes) -> Value {
    serde_json::from_slice(body).unwrap_or_else(|_| json!({}))
}

pub(super) fn header_str(headers: &HeaderMap, name: impl header::AsHeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}
