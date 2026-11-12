use super::*;

pub(crate) async fn list_bots(State(s): S) -> ApiResult<Response> {
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({ "bots": crate::bots::roster::list(&s, s.bots.list().await?).await, "now": unix_now() * 1000 })),
    )
        .into_response())
}

pub(crate) async fn create_bot(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let bot = s.bots.create(&parse(&body)?).await?;
    s.provision_now.notify_one();
    Ok(Json(bot))
}

pub(crate) async fn get_bot(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    s.bots
        .get(&slug)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("not found"))
}

pub(crate) async fn save_bot(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    Ok(Json(s.bots.save(&slug, &parse(&body)?, false).await?))
}

/// The person has this Bot's chat open: everything up to now is read.
pub(crate) async fn mark_seen(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    s.bots
        .save(&slug, &json!({ "lastSeenAt": vault::now_iso() }), false)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn delete_bot(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({ "ok": true, "bots": s.bots.remove(&slug).await? }),
    ))
}

pub(crate) async fn main_bot(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let slug = parse(&body)?["slug"].as_str().unwrap_or("").to_string();
    Ok(Json(json!({ "bots": s.bots.select(&slug).await? })))
}

pub(crate) async fn require_bot(s: &AppState, slug: &str) -> ApiResult<Value> {
    s.bots
        .get(slug)
        .await?
        .ok_or_else(|| ApiError::not_found("Bot not found"))
}

/// Starting points for New Bot.
pub(crate) async fn suggestions_get() -> Json<Value> {
    Json(json!({ "suggestions": crate::bots::suggestions::all() }))
}

/// Every provider T3 Code has ready, with its models and effort levels.
pub(crate) async fn providers_get(State(s): S) -> ApiResult<Json<Value>> {
    let list = s
        .providers
        .list(s.harness.as_ref())
        .await
        .map_err(|e| ApiError::new(502, format!("T3 didn't list providers: {e}")))?;
    Ok(Json(json!({ "ok": true, "providers": list["providers"] })))
}

/// A model choice from a request body, checked against T3's providers. When T3 can't be read
/// the old character check stands, so a choice is never lost to a hiccup.
pub(crate) async fn checked_selection(
    s: &AppState,
    body: &Value,
    current: &Value,
) -> ApiResult<Value> {
    let instance = body["instanceId"].as_str().unwrap_or("");
    let model = body["model"].as_str().unwrap_or("");
    let effort = body["effort"].as_str();
    // The character check first: it also refuses an empty choice.
    let plain = crate::bots::provisioner::model_selection(instance, model)
        .map_err(ApiError::bad_request)?;
    match s.providers.list(s.harness.as_ref()).await {
        Ok(list) => crate::harness::providers::selection(&list, instance, model, effort, current)
            .map_err(ApiError::bad_request),
        Err(_) => Ok(plain),
    }
}

pub(crate) async fn model_get(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    let bot = require_bot(&s, &slug).await?;
    let settings = s.settings().await;
    let current = [&bot["model"], &settings["defaultModel"]]
        .into_iter()
        .find(|m| m.is_object())
        .cloned()
        .unwrap_or_else(crate::bots::provisioner::default_model);
    Ok(Json(json!({
        "ok": true,
        "current": current["model"].as_str().unwrap_or(""),
        "instanceId": current["instanceId"].as_str().unwrap_or("claudeAgent"),
        "effort": crate::harness::providers::effort_of(&current),
        "runtimeMode": crate::bots::provisioner::launch_runtime_mode(&bot, &settings),
        "choices": crate::bots::provisioner::model_choices(&bot["model"], &settings["defaultModel"]),
    })))
}

pub(crate) async fn model_put(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let bot = require_bot(&s, &slug).await?;
    let body = parse(&body)?;
    let selection = checked_selection(&s, &body, &bot["model"]).await?;
    let mut patch = json!({ "model": selection });
    if body.get("runtimeMode").is_some() {
        let mode = body["runtimeMode"].as_str().unwrap_or("");
        if !matches!(mode, "approval-required" | "auto" | "full-access") {
            return Err(ApiError::bad_request(
                "runtimeMode must be approval-required, auto, or full-access",
            ));
        }
        patch["runtimeMode"] = json!(mode);
    }
    s.bots.save(&slug, &patch, false).await?;
    if s.harness.connected().await {
        if let Ok(provisioning) = s.harness.provisioning() {
            crate::bots::provisioner::push_model(provisioning, &bot, &selection)
                .await
                .map_err(|e| ApiError::new(502, format!("T3 didn't take the model: {e}")))?;
        }
    }
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn lineage_get(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    let thread = main_thread(&s, &slug).await?;
    let snapshot = s
        .harness
        .transcript()
        .map_err(|err| err.api())?
        .read(&thread)
        .await
        .map_err(|err| err.api_note(|msg| format!("T3 didn't return the thread: {msg}")))?;
    Ok(Json(json!({ "ok": true, "rows": snapshot.lineage() })))
}
