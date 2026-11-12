//! File upload and download: multipart parsing, range requests, byte replies.

use super::routes::{header_str, missing_reply, mutation, prelude, respond};
use super::state::Reply;
use super::Shared;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use serde_json::json;

pub(super) async fn upload_file(
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

pub(super) async fn file_content(
    State(state): State<Shared>,
    Path((_, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> axum::response::Response {
    file_bytes(&state, &id, headers.get(header::RANGE), false).await
}

pub(super) async fn file_head(
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
