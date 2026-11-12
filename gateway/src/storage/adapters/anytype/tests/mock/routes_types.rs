//! The single type: the `blocks` dataview read bootstrap verifies, and the
//! PATCH that replaces the type's property list.

use super::routes::{mutation, parse_json, prelude, respond};
use super::Shared;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;

pub(super) async fn get_type(
    State(state): State<Shared>,
    Path((_, type_key)): Path<(String, String)>,
) -> axum::response::Response {
    if let Some(response) = prelude(&state).await {
        return response;
    }
    let mock = state.lock().expect("mock");
    respond(mock.get_type(&type_key))
}

pub(super) async fn patch_type(
    State(state): State<Shared>,
    Path((_, type_key)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    let parsed = parse_json(&body);
    let path = format!("/types/{type_key}");
    mutation(&state, &headers, "PATCH", &path, &body, move |mock| {
        mock.patch_type(&type_key, &parsed)
    })
    .await
}
