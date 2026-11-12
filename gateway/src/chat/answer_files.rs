//! Files on a question answer.
//!
//! The person uploads through the existing attachment route first. The answer
//! names those ids under `attachmentsByQuestionId`. Each stored file becomes the
//! chat attachment record `message.dispatch` already uses (`type`, `id`, `name`,
//! `mimeType`, `sizeBytes`). The harness command keeps that map.

use crate::chat::attachments::{self, MAX_BYTES, MAX_FILES};
use crate::platform::error::{ApiError, ApiResult};
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::path::Path;

/// The question map from an answer body. `None` when the answer carries no files.
pub(crate) fn listed(body: &Value) -> ApiResult<Option<&Map<String, Value>>> {
    let Some(raw) = body.get("attachmentsByQuestionId") else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    let map = raw.as_object().ok_or_else(|| {
        ApiError::bad_request("attachmentsByQuestionId must list files by question")
    })?;
    for value in map.values() {
        if !value.is_array() {
            return Err(ApiError::bad_request("name each uploaded file"));
        }
    }
    let any = map
        .values()
        .any(|value| value.as_array().is_some_and(|ids| !ids.is_empty()));
    if any {
        Ok(Some(map))
    } else {
        Ok(None)
    }
}

/// The pending question with this id, or why the files cannot be sent.
pub(crate) fn open_question<'a>(
    transcript: &'a Value,
    request_id: &str,
) -> Result<&'a Value, String> {
    let requests = transcript
        .get("requests")
        .and_then(Value::as_array)
        .ok_or("That question is no longer waiting.")?;
    let request = requests
        .iter()
        .find(|request| request["id"].as_str() == Some(request_id))
        .ok_or("That question is no longer waiting.")?;
    match request["kind"].as_str() {
        Some("user_input_request") => Ok(request),
        _ => Err("Files go on a question.".into()),
    }
}

/// Stored uploads as chat attachment records, keyed by question id.
pub(crate) async fn records(
    root: &Path,
    slug: &str,
    listed: &Map<String, Value>,
    request: &Value,
) -> ApiResult<Value> {
    let questions = request["questions"].as_array();
    for question_id in listed.keys() {
        let on_request = questions.is_some_and(|questions| {
            questions
                .iter()
                .any(|question| question["id"].as_str() == Some(question_id.as_str()))
        });
        if !on_request {
            return Err(ApiError::bad_request(
                "That question is not on this request.",
            ));
        }
    }
    let mut seen = HashSet::new();
    let mut total = 0;
    let mut out = Map::new();
    for (question_id, value) in listed {
        let Some(ids) = value.as_array() else {
            return Err(ApiError::bad_request("name each uploaded file"));
        };
        let mut parts = Vec::new();
        for id in ids {
            let id = id
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| ApiError::bad_request("upload the file first"))?;
            if !seen.insert(id.to_string()) {
                return Err(ApiError::bad_request("that file is already attached"));
            }
            total += 1;
            if total > MAX_FILES {
                return Err(ApiError::bad_request(format!(
                    "attach at most {MAX_FILES} files"
                )));
            }
            parts.push(part(root, slug, id).await?);
        }
        if !parts.is_empty() {
            out.insert(question_id.clone(), json!(parts));
        }
    }
    Ok(Value::Object(out))
}

async fn part(root: &Path, slug: &str, id: &str) -> ApiResult<Value> {
    let (meta, bytes) = attachments::read_file(root, slug, id).await?;
    let name = attachments::clean_name(meta["name"].as_str().unwrap_or(""))?;
    let media = attachments::clean_media(meta["mediaType"].as_str().unwrap_or(""))?;
    if bytes.is_empty() {
        return Err(ApiError::bad_request("the file is empty"));
    }
    if bytes.len() > MAX_BYTES {
        return Err(ApiError::bad_request("the file is too large"));
    }
    Ok(attachments::command_part(&json!({
        "id": meta["id"],
        "name": name,
        "mediaType": media,
        "bytes": bytes.len(),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{respond_request, settings_state_with};
    use crate::harness::adapters::mock::MockHarness;
    use crate::harness::{Approval, Harness};
    use axum::body::Bytes;
    use axum::extract::{Path, State};
    use axum::http::HeaderMap;
    use axum::Json;
    use serde_json::json;
    use std::sync::Arc;

    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        headers
    }

    fn question(status: &str) -> Value {
        json!({
            "turnItems": [{
                "id": "item-2",
                "requestId": "req-2",
                "type": "user_input_request",
                "status": status,
                "questions": [{
                    "id": "q1",
                    "header": "File",
                    "question": "Which notes?",
                    "allowCustomAnswer": true
                }]
            }]
        })
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

    async fn answer(
        state: &Arc<crate::AppState>,
        slug: &str,
        body: Value,
    ) -> Result<Value, crate::platform::error::ApiError> {
        let Json(value) = respond_request(
            State(Arc::clone(state)),
            Path((slug.to_string(), "req-2".to_string())),
            headers(),
            Bytes::from(body.to_string()),
        )
        .await?;
        Ok(value)
    }

    #[tokio::test]
    async fn a_question_answer_sends_the_uploaded_file_with_the_answer() {
        let dir = tempfile::tempdir().unwrap();
        let mock = Arc::new(MockHarness::new());
        mock.set_projection("thread-ada", question("pending"));
        let state = settings_state_with(dir.path(), Some(Arc::clone(&mock) as Arc<dyn Harness>));
        let slug = ada(&state).await;
        let meta = attachments::write_file(
            &state.cfg.bots_root,
            &slug,
            "notes.txt",
            "text/plain",
            b"hello",
        )
        .await
        .unwrap();
        let id = meta["id"].as_str().unwrap();
        answer(
            &state,
            &slug,
            json!({
                "kind": "user_input_request",
                "answers": { "q1": "the notes" },
                "attachmentsByQuestionId": { "q1": [id] }
            }),
        )
        .await
        .unwrap();
        let Approval::Request {
            answers,
            attachments_by_question,
            ..
        } = &mock.approvals()[0]
        else {
            panic!("a question answer is a request");
        };
        assert_eq!(answers.as_ref().unwrap()["q1"], "the notes");
        let file = &attachments_by_question.as_ref().unwrap()["q1"][0];
        assert_eq!(file["type"], "file");
        assert_eq!(file["id"], id);
        assert_eq!(file["name"], "notes.txt");
        assert_eq!(file["mimeType"], "text/plain");
        assert_eq!(file["sizeBytes"], 5);
        assert!(file.get("data").is_none());
    }

    #[tokio::test]
    async fn size_type_missing_upload_and_a_stale_request_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mock = Arc::new(MockHarness::new());
        mock.set_projection("thread-ada", question("pending"));
        let state = settings_state_with(dir.path(), Some(Arc::clone(&mock) as Arc<dyn Harness>));
        let slug = ada(&state).await;
        let root = &state.cfg.bots_root;

        let huge = vec![0_u8; MAX_BYTES + 1];
        let big =
            attachments::write_file(root, &slug, "big.bin", "application/octet-stream", &huge)
                .await
                .unwrap();
        let too_big = answer(
            &state,
            &slug,
            json!({
                "kind": "user_input_request",
                "answers": { "q1": "x" },
                "attachmentsByQuestionId": { "q1": [big["id"]] }
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(too_big.status, axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(too_big.message, "the file is too large");

        let html = attachments::write_file(root, &slug, "page.html", "text/html", b"<p>hi</p>")
            .await
            .unwrap();
        let bad_type = answer(
            &state,
            &slug,
            json!({
                "kind": "user_input_request",
                "answers": { "q1": "x" },
                "attachmentsByQuestionId": { "q1": [html["id"]] }
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(bad_type.message, "that file type is not allowed");

        let missing = answer(
            &state,
            &slug,
            json!({
                "kind": "user_input_request",
                "answers": { "q1": "x" },
                "attachmentsByQuestionId": { "q1": ["11111111-1111-1111-1111-111111111111"] }
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(missing.status, axum::http::StatusCode::NOT_FOUND);
        assert_eq!(missing.message, "no such file");

        mock.set_projection("thread-ada", question("completed"));
        let notes = attachments::write_file(root, &slug, "notes.txt", "text/plain", b"hello")
            .await
            .unwrap();
        let stale = answer(
            &state,
            &slug,
            json!({
                "kind": "user_input_request",
                "answers": { "q1": "x" },
                "attachmentsByQuestionId": { "q1": [notes["id"]] }
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(stale.status, axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(stale.message, "That question is no longer waiting.");
        assert!(mock.approvals().is_empty());
    }
}
