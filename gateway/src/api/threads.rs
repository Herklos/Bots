use super::*;

pub(crate) async fn thread_meta_get(
    State(s): S,
    Path(thread): Path<String>,
) -> ApiResult<Json<Value>> {
    Ok(Json(s.threads.get(Some(&thread)).await?))
}

pub(crate) async fn thread_reaction(
    State(s): S,
    Path(thread): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    Ok(Json(
        s.threads
            .toggle_reaction(Some(&thread), body["item"].as_str(), body["emoji"].as_str())
            .await?,
    ))
}

pub(crate) async fn thread_pin(
    State(s): S,
    Path(thread): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    Ok(Json(
        s.threads
            .toggle_pin(Some(&thread), body["item"].as_str())
            .await?,
    ))
}

pub(crate) async fn thread_reply(
    State(s): S,
    Path(thread): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    Ok(Json(
        s.threads
            .set_reply(
                Some(&thread),
                body["messageId"].as_str(),
                body["replyTo"].as_str(),
                body["text"].as_str(),
            )
            .await?,
    ))
}

pub(crate) async fn thread_read(
    State(s): S,
    Path(thread): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    Ok(Json(
        s.threads
            .mark_read(
                Some(&thread),
                body["latest"].as_str(),
                body["open"].as_bool().unwrap_or(false),
                body["hidden"].as_bool().unwrap_or(false),
            )
            .await?,
    ))
}

pub(crate) async fn thread_voice(
    State(s): S,
    Path(thread): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let at = body["at"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(crate::chat::routines::stamped);
    Ok(Json(
        s.threads
            .add_voice_call(Some(&thread), body.get("seconds"), &at)
            .await?,
    ))
}

pub(crate) async fn respond_request(
    State(s): S,
    Path((slug, id)): Path<(String, String)>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let thread =
        resolve_chat_thread(&s, &slug, body["child"].as_str(), body["side"].as_str()).await?;
    let kind = body["kind"].as_str().unwrap_or("");
    let attachments = question_files(&s, &slug, &thread, &id, &body).await?;
    let outbound = crate::chat::approvals::outbound(kind, &thread, &id, &body, attachments)
        .map_err(ApiError::bad_request)?;
    Ok(Json(
        crate::chat::approvals::send(s.harness.approvals().map_err(|err| err.api())?, outbound)
            .await?,
    ))
}

async fn question_files(
    s: &AppState,
    slug: &str,
    thread: &str,
    request_id: &str,
    body: &Value,
) -> ApiResult<Option<Value>> {
    let Some(listed) = crate::chat::answer_files::listed(body)? else {
        return Ok(None);
    };
    let snapshot = s
        .harness
        .transcript()
        .map_err(|err| err.api())?
        .read(thread)
        .await
        .map_err(|err| err.api())?;
    let transcript = snapshot.transcript();
    let request = crate::chat::answer_files::open_question(&transcript, request_id)
        .map_err(ApiError::bad_request)?;
    Ok(Some(
        crate::chat::answer_files::records(&s.cfg.bots_root, slug, listed, request).await?,
    ))
}

pub(crate) async fn thread_previews(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Response> {
    require_json(&h)?;
    let b = parse(&body)?;
    let names: HashMap<String, String> = s
        .bots
        .list()
        .await?
        .iter()
        .filter_map(|bot| {
            Some((
                bot["mainThreadId"].as_str()?.to_string(),
                bot["name"].as_str()?.to_string(),
            ))
        })
        .collect();
    let harness = s.harness.clone();
    let preview_names = names.clone();
    let previews = s
        .threads
        .previews(b.get("stamps"), &names, |thread| {
            let harness = harness.clone();
            let preview_names = preview_names.clone();
            async move {
                harness
                    .previews()
                    .map_err(|err| anyhow::anyhow!(err.to_string()))?
                    .inbox(&thread, &preview_names)
                    .await
                    .map_err(|err| anyhow::anyhow!(err.to_string()))
            }
        })
        .await?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({ "previews": previews })),
    )
        .into_response())
}
