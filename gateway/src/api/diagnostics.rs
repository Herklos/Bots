//! Settings diagnostics. The report is the same redacted text as `hbots doctor`.
//! Ask a Bot sends that text as a file through the existing message path.

use super::*;

const ASK: &str = "Look at the attached diagnostics and the troubleshooting section of DEV.md. Say what is wrong and what to try. Do not post the report.";

pub(crate) async fn get_diagnostics(Query(q): Query<HashMap<String, String>>) -> Response {
    let (text, value) = crate::platform::cli::bundle().await;
    if q.contains_key("download") {
        return (
            StatusCode::OK,
            [
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/plain; charset=utf-8"),
                ),
                (
                    header::CONTENT_DISPOSITION,
                    HeaderValue::from_static("attachment; filename=\"hbots-diagnostics.txt\""),
                ),
            ],
            text,
        )
            .into_response();
    }
    Json(json!({ "ok": true, "text": text, "report": value })).into_response()
}

pub(crate) async fn ask_diagnostics(
    State(s): S,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let slug = body["slug"].as_str().unwrap_or("").trim();
    if slug.is_empty() {
        return Err(ApiError::bad_request("Name a Bot."));
    }
    let bot = require_bot(&s, slug).await?;
    let thread = main_thread(&s, slug).await?;
    crate::bots::provisioner::limit_existing_thread(&s, &bot, &thread)
        .await
        .map_err(|err| err.api())?;
    let (text, _) = crate::platform::cli::bundle().await;
    let saved = crate::chat::attachments::write_file(
        &s.cfg.bots_root,
        slug,
        "hbots-diagnostics.txt",
        "text/plain",
        text.as_bytes(),
    )
    .await?;
    let part = crate::chat::attachments::command_part(&saved);
    let turns = s.harness.turns().map_err(|err| err.api())?;
    turns
        .send_user(&crate::harness::UserMessage::new(
            &thread,
            &crate::harness::message_id(None),
            ASK,
            &[part],
        ))
        .await
        .map_err(|err| err.api())?;
    s.usage
        .note_for(slug, crate::bots::usage::Event::Message)
        .await;
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::super::tests::settings_state_with;
    use super::*;
    use crate::harness::adapters::mock::MockHarness;
    use crate::harness::Harness;

    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
        headers
    }

    #[tokio::test]
    async fn the_report_is_redacted_text() {
        let response = get_diagnostics(Query(HashMap::new())).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["ok"], true);
        assert!(body["text"].as_str().unwrap().contains("hbots"));
        assert!(body["report"]["version"].is_string());
        if let Ok(token) = std::env::var("T3_TOKEN") {
            if token.len() > 8 {
                let raw = body.to_string();
                assert!(!raw.contains(&token));
                assert!(!body["text"].as_str().unwrap().contains(&token));
            }
        }
    }

    #[tokio::test]
    async fn ask_sends_the_report_as_a_text_file() {
        let dir = tempfile::tempdir().unwrap();
        let mock = Arc::new(MockHarness::new());
        let state = settings_state_with(dir.path(), Some(Arc::clone(&mock) as Arc<dyn Harness>));
        let created = state.bots.create(&json!({ "name": "Ada" })).await.unwrap();
        let slug = created["slug"].as_str().unwrap().to_string();
        let missing = ask_diagnostics(
            State(state.clone()),
            headers(),
            Bytes::from(json!({ "slug": slug }).to_string()),
        )
        .await
        .unwrap_err();
        assert_eq!(missing.status, StatusCode::CONFLICT);

        state
            .bots
            .save(&slug, &json!({ "mainThreadId": "thread-ada" }), false)
            .await
            .unwrap();
        let sent = ask_diagnostics(
            State(state.clone()),
            headers(),
            Bytes::from(json!({ "slug": slug }).to_string()),
        )
        .await
        .unwrap();
        assert_eq!(sent.0["ok"], true);
        let messages = mock.messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].thread_id, "thread-ada");
        assert!(messages[0].text.contains("DEV.md"));
        assert!(messages[0].text.contains("Do not post the report."));
        assert_eq!(messages[0].attachments.len(), 1);
        assert_eq!(messages[0].attachments[0]["name"], "hbots-diagnostics.txt");
        assert_eq!(messages[0].attachments[0]["mimeType"], "text/plain");
    }
}
