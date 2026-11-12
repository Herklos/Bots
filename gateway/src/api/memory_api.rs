//! `GET` and `PUT /api/memory`. The person reads and saves Markdown memory.
//! A Bot save rewrites that Bot's managed block. A shared save waits for the poller.

use super::*;
use crate::bots::memory_files::{read_file, save_file, FileScope, MemoryFile, SaveFail};
use std::io;

#[derive(Debug)]
enum ScopeError {
    Invalid(&'static str),
    Failed(ApiError),
}

pub(crate) async fn memory_get(
    State(s): S,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let (scope, slug) = match scope_of(&s, &q).await {
        Ok(pair) => pair,
        Err(ScopeError::Invalid(message)) => return Ok(invalid(message)),
        Err(ScopeError::Failed(err)) => return Err(err),
    };
    match read_file(&s.memory, scope, &slug).await {
        Ok(file) => Ok(read_response(&file)),
        Err(err) => file_error(err),
    }
}

pub(crate) async fn memory_put(
    State(s): S,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    require_json(&headers)?;
    let (scope, slug) = match scope_of(&s, &q).await {
        Ok(pair) => pair,
        Err(ScopeError::Invalid(message)) => return Ok(invalid(message)),
        Err(ScopeError::Failed(err)) => return Err(err),
    };
    let body = parse(&body)?;
    let (Some(text), Some(revision)) = (body["text"].as_str(), body["revision"].as_str()) else {
        return Ok(invalid("text and revision are required."));
    };
    match save_file(&s.memory, scope, &slug, text, revision).await {
        Ok(file) => {
            if scope == FileScope::Bot {
                s.bots.refresh_agents(&slug).await?;
            }
            Ok(saved_response(&file))
        }
        Err(SaveFail::Invalid(message)) => Ok(invalid(&message)),
        Err(SaveFail::Conflict(file)) => Ok(conflict_response(&file)),
        Err(SaveFail::Io(err)) => file_error(err),
    }
}

async fn scope_of(
    s: &AppState,
    q: &HashMap<String, String>,
) -> Result<(FileScope, String), ScopeError> {
    let Some(scope) = q.get("scope").and_then(|value| FileScope::parse(value)) else {
        return Err(ScopeError::Invalid("scope must be bot or shared."));
    };
    let slug = match scope {
        FileScope::Shared => String::new(),
        FileScope::Bot => {
            let slug = q.get("slug").cloned().unwrap_or_default();
            if slug.is_empty() {
                return Err(ScopeError::Invalid("A Bot slug is required."));
            }
            require_bot(s, &slug).await.map_err(ScopeError::Failed)?;
            slug
        }
    };
    Ok((scope, slug))
}

fn read_response(file: &MemoryFile) -> Response {
    (
        StatusCode::OK,
        Json(json!({
            "scope": file.scope.as_str(),
            "path": file.path,
            "text": file.text,
            "revision": file.revision,
            "truncated": file.truncated,
        })),
    )
        .into_response()
}

fn saved_response(file: &MemoryFile) -> Response {
    (
        StatusCode::OK,
        Json(json!({
            "revision": file.revision,
            "text": file.text,
        })),
    )
        .into_response()
}

fn conflict_response(file: &MemoryFile) -> Response {
    coded(
        StatusCode::CONFLICT,
        json!({
            "code": "conflict",
            "revision": file.revision,
            "text": file.text,
            "message": "This file changed since you opened it.",
        }),
    )
}

fn invalid(message: &str) -> Response {
    coded(
        StatusCode::BAD_REQUEST,
        json!({ "code": "invalid", "message": message }),
    )
}

fn coded(status: StatusCode, value: Value) -> Response {
    (status, [("x-content-type-options", "nosniff")], Json(value)).into_response()
}

fn file_error(err: io::Error) -> Result<Response, ApiError> {
    if err.kind() == io::ErrorKind::InvalidInput {
        let message = err.to_string();
        if message.contains("symlink") || message == "directory" {
            return Ok(invalid(
                "This memory file is a link or a folder, so hbots will not read or replace it.",
            ));
        }
        return Err(ApiError::bad_request(message));
    }
    Err(err.into())
}
