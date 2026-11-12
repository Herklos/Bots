use super::*;

pub(crate) async fn main_thread(s: &AppState, slug: &str) -> ApiResult<String> {
    let bot = s
        .bots
        .get(slug)
        .await?
        .ok_or_else(|| ApiError::not_found("Bot not found"))?;
    bot["mainThreadId"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| ApiError::new(409, "This Bot has no conversation yet"))
}

/// Rows of the parent's thread family. Used only to decide which child a send may use.
pub(crate) async fn parent_rows(s: &AppState, parent: &str) -> ApiResult<Vec<Value>> {
    let snapshot = s
        .harness
        .transcript()
        .map_err(|err| err.api())?
        .read(parent)
        .await
        .map_err(|err| err.api())?;
    Ok(snapshot.lineage())
}

/// Parent thread, or a resolved local child. A remote child or the parent id is an error
/// and nothing has been written yet.
pub(crate) async fn resolve_chat_thread(
    s: &AppState,
    slug: &str,
    child: Option<&str>,
    side: Option<&str>,
) -> ApiResult<String> {
    let (child, side) = crate::chat::side::lane_ids(child, side)?;
    let parent = main_thread(s, slug).await?;
    if let Some(id) = side {
        return crate::chat::side::require_side(s.harness.as_ref(), &parent, id).await;
    }
    if child.is_none() {
        return Ok(parent);
    }
    let rows = parent_rows(s, &parent).await?;
    crate::bots::lineage::thread_for(&rows, &parent, child).map_err(ApiError::bad_request)
}

/// `send_body` is the 200. A match that did not change is a 304 with an empty body and the same tag.
pub(crate) fn with_etag(tag: &str, body: Value, send_body: bool) -> Response {
    let etag = HeaderValue::from_str(tag).unwrap_or(HeaderValue::from_static("\"0\""));
    let cache = HeaderValue::from_static("no-store");
    if !send_body {
        return (
            StatusCode::NOT_MODIFIED,
            [(header::ETAG, etag), (header::CACHE_CONTROL, cache)],
        )
            .into_response();
    }
    (
        StatusCode::OK,
        [(header::ETAG, etag), (header::CACHE_CONTROL, cache)],
        Json(body),
    )
        .into_response()
}

pub(crate) async fn chat_view(
    s: &AppState,
    slug: &str,
    child: Option<&str>,
    side: Option<&str>,
) -> ApiResult<Value> {
    let (child, side) = crate::chat::side::lane_ids(child, side)?;
    let thread = resolve_chat_thread(s, slug, child, side).await?;
    let on_main = child.is_none() && side.is_none();
    let transcript = s.harness.transcript().map_err(|err| err.api())?;
    let read_only = false;
    let snapshot = transcript.read(&thread).await.map_err(|err| err.api())?;
    let listed = s.bots.list().await?;
    let hidden = listed
        .iter()
        .any(|b| b["slug"].as_str() == Some(slug) && b["hidden"] == true);
    let peers: chat::Peers = listed
        .iter()
        .filter_map(|b| {
            Some((
                b["mainThreadId"].as_str()?.to_string(),
                json!({ "slug": b["slug"], "name": b["name"], "color": b["color"], "shape": b["shape"] }),
            ))
        })
        .collect();
    // Opening the Bot's own chat advances last-read to the message id the inbox stored.
    // A child thread is not that chat, and a visible Bot keeps the roster's own mark.
    if on_main && hidden {
        if let Ok(meta) = s.threads.get(Some(&thread)).await {
            let stored = meta["lastRead"].as_str().unwrap_or("");
            let latest = meta["latest"].as_str().unwrap_or("");
            if !latest.is_empty() && stored != latest {
                let _ = s.threads.mark_read(Some(&thread), None, true, true).await;
            }
        }
    }
    let mut out = snapshot.transcript_for(&peers);
    out["threadId"] = json!(thread);
    out["readOnly"] = json!(read_only);
    out["hidden"] = json!(hidden);
    let pins = match s.threads.get(Some(&thread)).await {
        Ok(meta) => meta.get("pins").cloned().unwrap_or(json!([])),
        Err(_) => json!([]),
    };
    out["pins"] = pins;
    let root = listed
        .iter()
        .find(|bot| bot["slug"].as_str() == Some(slug))
        .and_then(|bot| bot["workspaceRoot"].as_str())
        .unwrap_or("");
    // Library images are checked here, after the transcript is built. A cached
    // check does no file IO, so the 500 ms poll stays cheap.
    crate::chat::images::attach(
        &mut out,
        &s.images,
        s.workspace.as_ref(),
        slug,
        root,
        crate::chat::images::now_ms(),
    )
    .await;
    // The client has no clock it may read in its data code, so the gateway says what time it is.
    // The tag ignores this field: a body that only moves `now` is not a new transcript.
    out["now"] = json!(unix_now() * 1000);
    Ok(out)
}

pub(crate) async fn read_chat(
    State(s): S,
    Path(slug): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let child = q.get("child").map(String::as_str);
    let side = q.get("side").map(String::as_str);
    let (child, side) = crate::chat::side::lane_ids(child, side)?;
    let view = chat_view(&s, &slug, child, side).await?;
    let tag = chat::transcript_tag(&view);
    let seen = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok());
    if !chat::none_match(seen, &tag) {
        return Ok(with_etag(&tag, view, true));
    }
    let key = format!(
        "chat\n{slug}\n{}\n{}",
        side.map(|_| "side")
            .unwrap_or(child.map(|_| "child").unwrap_or("")),
        side.or(child).unwrap_or("")
    );
    let outcome = s
        .holds
        .hold_until_change(&key, &tag, chat::LOOK_EVERY, chat::HOLD_FOR, || {
            let s = s.clone();
            let slug = slug.clone();
            let child = child.map(str::to_string);
            let side = side.map(str::to_string);
            async move {
                match chat_view(&s, &slug, child.as_deref(), side.as_deref()).await {
                    Ok(next) => {
                        let next_tag = chat::transcript_tag(&next);
                        Some((next, next_tag))
                    }
                    Err(_) => None,
                }
            }
        })
        .await;
    match outcome {
        chat::Hold::Changed(next) => Ok(with_etag(&chat::transcript_tag(&next), next, true)),
        chat::Hold::Same | chat::Hold::Superseded | chat::Hold::Stopped => {
            Ok(with_etag(&tag, Value::Null, false))
        }
    }
}

pub(crate) async fn send_chat(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let text = body["text"]
        .as_str()
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    let files = crate::chat::attachments::for_send(&s.cfg.bots_root, &slug, &body).await?;
    if text.is_empty() && files.is_empty() {
        return Err(ApiError::bad_request("Write a message first."));
    }
    // Resolve before any reply is stored or any command is dispatched. A refused child
    // or side id builds no command, so the parent thread is not written.
    let message_id = crate::harness::message_id(body["messageId"].as_str());
    let thread =
        resolve_chat_thread(&s, &slug, body["child"].as_str(), body["side"].as_str()).await?;
    let bot = require_bot(&s, &slug).await?;
    crate::bots::provisioner::limit_existing_thread(&s, &bot, &thread)
        .await
        .map_err(|err| err.api())?;
    let turns = s.harness.turns().map_err(|err| err.api())?;
    if let Some(reply_to) = body["replyTo"].as_str().filter(|id| !id.is_empty()) {
        s.threads
            .set_reply(
                Some(&thread),
                Some(&message_id),
                Some(reply_to),
                body["quote"].as_str(),
            )
            .await?;
    }
    turns
        .send_user(&crate::harness::UserMessage::new(
            &thread,
            &message_id,
            &text,
            &files,
        ))
        .await
        .map_err(|err| err.api())?;
    s.usage
        .note_for(&slug, crate::bots::usage::Event::Message)
        .await;
    Ok(Json(json!({ "ok": true, "messageId": message_id })))
}

pub(crate) async fn send_voice(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let seconds = body
        .get("seconds")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .ok_or_else(|| ApiError::bad_request("invalid duration"))?;
    let thread =
        resolve_chat_thread(&s, &slug, body["child"].as_str(), body["side"].as_str()).await?;
    let attachment_id = body["attachmentId"]
        .as_str()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string);
    let (stored, wrote) = if let Some(id) = attachment_id {
        (
            chat::dispatch_stored_voice(
                &s.cfg.bots_root,
                &slug,
                &s.threads,
                &thread,
                &id,
                seconds,
                &crate::chat::routines::stamped(),
            )
            .await?,
            false,
        )
    } else {
        #[allow(
            clippy::map_err_ignore,
            reason = "a base64 error can quote the clip, which is the user's audio"
        )]
        let bytes = body["data"]
            .as_str()
            .map(|data| {
                base64::engine::Engine::decode(&base64::engine::general_purpose::STANDARD, data)
            })
            .transpose()
            .map_err(|_| ApiError::bad_request("That clip could not be read."))?
            .unwrap_or_default();
        if bytes.is_empty() {
            return Err(ApiError::bad_request("That clip is empty."));
        }
        (
            chat::store_voice_send(
                &s.cfg.bots_root,
                &slug,
                &s.threads,
                &thread,
                &bytes,
                seconds,
                &crate::chat::routines::stamped(),
            )
            .await?,
            true,
        )
    };
    let command = &stored["command"];
    let bot = require_bot(&s, &slug).await?;
    crate::bots::provisioner::limit_existing_thread(
        &s,
        &bot,
        command["threadId"].as_str().unwrap_or(&thread),
    )
    .await
    .map_err(|err| err.api())?;
    s.harness
        .turns()
        .map_err(|err| err.api())?
        .send_user(&crate::harness::UserMessage::new(
            command["threadId"].as_str().unwrap_or(&thread),
            command["messageId"].as_str().unwrap_or(""),
            command["text"].as_str().unwrap_or(""),
            command["attachments"].as_array().unwrap_or(&Vec::new()),
        ))
        .await
        .map_err(|err| err.api())?;
    s.usage
        .note_for(&slug, crate::bots::usage::Event::Message)
        .await;
    if wrote {
        s.usage
            .note_for(&slug, crate::bots::usage::Event::Attachment)
            .await;
    }
    Ok(Json(json!({
        "ok": true,
        "attachment": stored["attachment"],
        "voiceCalls": stored["voiceCalls"],
        "messageId": stored["command"]["messageId"],
    })))
}

pub(crate) async fn stop_chat(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let thread =
        resolve_chat_thread(&s, &slug, body["child"].as_str(), body["side"].as_str()).await?;
    let snapshot = s
        .harness
        .transcript()
        .map_err(|err| err.api())?
        .read(&thread)
        .await
        .map_err(|err| err.api())?;
    let Some(run) = snapshot.active_run_id() else {
        return Ok(Json(json!({ "ok": true, "stopped": false })));
    };
    s.harness
        .turns()
        .map_err(|err| err.api())?
        .interrupt(&thread, &run)
        .await
        .map_err(|err| err.api())?;
    Ok(Json(json!({ "ok": true, "stopped": true })))
}
