use super::*;

fn routines(s: &AppState) -> ApiResult<&dyn crate::harness::Routines> {
    s.harness.routines().map_err(|err| err.api())
}

pub(crate) async fn routines_list(State(s): S) -> ApiResult<Json<Value>> {
    let mut listed = crate::chat::routines::list(routines(&s)?).await?;
    if let Some(obj) = listed.as_object_mut() {
        obj.insert("history".into(), s.routine_history.recent().await);
    }
    Ok(Json(listed))
}

pub(crate) async fn routines_upsert(
    State(s): S,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let mut task = parse(&body)?;
    let cap =
        crate::bots::runtime_mode::RuntimeMode::cap(s.settings().await["maxRuntimeMode"].as_str());
    crate::chat::routines::clamp_task_mode(&mut task, cap);
    Ok(Json(
        crate::chat::routines::change(routines(&s)?, &s.routine_history, "upsert", task).await?,
    ))
}

pub(crate) async fn routines_enabled(
    State(s): S,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    Ok(Json(
        crate::chat::routines::change(routines(&s)?, &s.routine_history, "enabled", parse(&body)?)
            .await?,
    ))
}

pub(crate) async fn routines_delete(
    State(s): S,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    Ok(Json(
        crate::chat::routines::change(routines(&s)?, &s.routine_history, "delete", parse(&body)?)
            .await?,
    ))
}

pub(crate) async fn routines_run(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    Ok(Json(
        crate::chat::routines::change(routines(&s)?, &s.routine_history, "run", parse(&body)?)
            .await?,
    ))
}

pub(crate) async fn bot_routines(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    let bot = require_bot(&s, &slug).await?;
    let Some(thread) = bot["mainThreadId"].as_str().filter(|t| !t.is_empty()) else {
        return Ok(Json(json!({
            "ok": true,
            "tasks": [],
            "error": "This Bot is still getting set up.",
        })));
    };
    let routines = routines(&s)?;
    let listed = crate::chat::routines::list(routines)
        .await
        .map_err(|e| ApiError::new(502, format!("T3 didn't list routines: {}", e.message)))?;
    let mut tasks: Vec<Value> = crate::chat::routines::tasks_for_thread(&listed, thread)
        .iter()
        .map(crate::chat::routines::task_row)
        .collect();
    // The chip reads the newest delivery. A failed read leaves it blank.
    crate::chat::routines::inspect::attach_outcomes(routines, &mut tasks).await;
    Ok(Json(json!({ "ok": true, "tasks": tasks, "error": "" })))
}

pub(crate) async fn bot_routine_save(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let bot = require_bot(&s, &slug).await?;
    let body = parse(&body)?;
    let is_hook = body["kind"].as_str() == Some("webhook");
    let cap =
        crate::bots::runtime_mode::RuntimeMode::cap(s.settings().await["maxRuntimeMode"].as_str());
    let mut task = if is_hook {
        crate::chat::routines::webhook::new_webhook_routine(&bot, &body, unix_now())
            .map_err(ApiError::bad_request)?
    } else {
        crate::chat::routines::new_routine(
            &bot,
            body["title"].as_str().unwrap_or(""),
            body["kind"].as_str().unwrap_or("hour"),
            unix_now(),
            cap,
        )
        .map_err(ApiError::bad_request)?
    };
    crate::chat::routines::clamp_task_mode(&mut task, cap);
    let saved =
        crate::chat::routines::change(routines(&s)?, &s.routine_history, "upsert", task).await?;
    let task = saved.get("task").cloned().unwrap_or_else(|| saved.clone());
    let mut out = json!({ "ok": true, "task": crate::chat::routines::task_row(&task) });
    // The address is returned only here, on create, and on the hook routes.
    if is_hook {
        let settings = s.settings().await;
        out["hook"] = crate::chat::routines::inspect::hook_view(&task, &settings, &s.remote);
    }
    Ok(Json(out))
}

/// Local self-delivery for the URL card's Test button. The sample body goes to
/// T3's hook endpoint with this machine's own URL, so it behaves like an outside
/// sender. The secret is signed into a header and then dropped: it is not stored.
pub(crate) async fn bot_routine_test(
    State(s): S,
    Path((slug, id)): Path<(String, String)>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let task = owned_task(&s, &slug, &id).await?;
    require_webhook(&task)?;
    let input = parse(&body)?;
    let payload = match input.get("body") {
        Some(Value::String(text)) => text,
        None => return Err(ApiError::bad_request("Give the test a sample body.")),
        Some(_) => return Err(ApiError::bad_request("A test body is text.")),
    };
    if payload.len() > super::hooks_api::BODY_LIMIT {
        return Err(ApiError::bad_request("A test body is at most 1 MiB."));
    }
    let signature = match secret_of(&input) {
        None => None,
        // A secret typed here is used once for this request and then dropped.
        Some(secret) => Some(sign(&task, payload.as_bytes(), secret.as_bytes())?),
    };
    let Some(path) = task["webhook"]["path"].as_str().filter(|p| !p.is_empty()) else {
        return Err(ApiError::bad_request("This routine has no address yet."));
    };
    let base = s.cfg.t3_url.trim_end_matches('/');
    let url = format!("{base}{path}");
    let mut request = super::hooks_api::http_client()
        .post(&url)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some((name, value)) = &signature {
        request = request.header(name.as_str(), value.as_str());
    }
    // The reqwest error can quote the token URL, so it is discarded.
    let outcome = request
        .body(payload.as_bytes().to_vec())
        .send()
        .await
        .map_err(|_send| ApiError::new(502, "T3 did not take the delivery."))?;
    let ok = outcome.status().is_success();
    let made = outcome
        .headers()
        .get("x-t3-hook-outcome")
        .and_then(|value| value.to_str().ok())
        .unwrap_or(if ok { "accepted" } else { "" })
        .to_string();
    let _ = outcome.bytes().await;
    Ok(Json(json!({ "ok": ok, "outcome": made })))
}

fn secret_of(input: &Value) -> Option<String> {
    match input.get("secret") {
        None | Some(Value::Null) => None,
        Some(Value::String(secret)) if secret.is_empty() => None,
        Some(Value::String(secret)) => Some(secret.clone()),
        Some(_) => None,
    }
}

/// Plain HMAC-SHA256 over the raw body, encoded the way the task's signature says.
fn sign(task: &Value, payload: &[u8], secret: &[u8]) -> ApiResult<(String, String)> {
    use base64::Engine;
    use hmac::Mac;
    let sig = &task["schedule"]["signature"];
    let Some(header) = sig["header"].as_str().filter(|h| !h.is_empty()) else {
        return Err(ApiError::bad_request(
            "This webhook has no signature check.",
        ));
    };
    let mut mac = <hmac::Hmac<sha2::Sha256>>::new_from_slice(secret)
        .map_err(|_key| ApiError::bad_request("A signing secret is text."))?;
    mac.update(payload);
    let digest = mac.finalize().into_bytes();
    let raw = match sig["encoding"].as_str().unwrap_or("hex") {
        "base64" => base64::engine::general_purpose::STANDARD.encode(digest),
        _ => hex::encode(digest),
    };
    let prefix = sig["prefix"].as_str().unwrap_or("");
    Ok((header.to_ascii_lowercase(), format!("{prefix}{raw}")))
}

pub(crate) async fn owned_task(s: &AppState, slug: &str, id: &str) -> ApiResult<Value> {
    let bot = require_bot(s, slug).await?;
    let thread = bot["mainThreadId"].as_str().unwrap_or("");
    if thread.is_empty() {
        return Err(ApiError::not_found("That routine is not on this Bot."));
    }
    let listed = crate::chat::routines::list(routines(s)?).await?;
    crate::chat::routines::tasks_for_thread(&listed, thread)
        .into_iter()
        .find(|task| task["id"] == id)
        .ok_or_else(|| ApiError::not_found("That routine is not on this Bot."))
}

pub(crate) async fn owned_routine(s: &AppState, slug: &str, id: &str) -> ApiResult<()> {
    owned_task(s, slug, id).await.map(|_| ())
}

fn require_webhook(task: &Value) -> ApiResult<()> {
    if crate::chat::routines::webhook::kind_of(task) == "webhook" {
        Ok(())
    } else {
        Err(ApiError::bad_request("That routine is not a webhook."))
    }
}

fn delivery_miss(err: crate::harness::HarnessError) -> ApiError {
    if let crate::harness::HarnessError::Failed(msg) = &err {
        if msg.to_ascii_lowercase().contains("not found") {
            return ApiError::not_found("That delivery is not on this webhook.");
        }
    }
    err.api()
}

async fn hook_json(s: &AppState, task: &Value) -> Json<Value> {
    let settings = s.settings().await;
    Json(crate::chat::routines::inspect::hook_view(
        task, &settings, &s.remote,
    ))
}

pub(crate) async fn bot_hook(
    State(s): S,
    Path((slug, id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let task = owned_task(&s, &slug, &id).await?;
    require_webhook(&task)?;
    Ok(hook_json(&s, &task).await)
}

pub(crate) async fn bot_rotate(
    State(s): S,
    Path((slug, id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let task = owned_task(&s, &slug, &id).await?;
    require_webhook(&task)?;
    let rotated = routines(&s)?
        .rotate_webhook_token(&id)
        .await
        .map_err(crate::harness::HarnessError::api)?;
    let fresh = owned_task(&s, &slug, &id).await?;
    let task = crate::chat::routines::inspect::task_after_rotate(fresh, &rotated);
    Ok(hook_json(&s, &task).await)
}

pub(crate) async fn bot_deliveries(
    State(s): S,
    Path((slug, id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let task = owned_task(&s, &slug, &id).await?;
    require_webhook(&task)?;
    let listed = routines(&s)?
        .list_deliveries(&id)
        .await
        .map_err(crate::harness::HarnessError::api)?;
    Ok(Json(crate::chat::routines::inspect::delivery_list(&listed)))
}

pub(crate) async fn bot_delivery(
    State(s): S,
    Path((slug, id, delivery_id)): Path<(String, String, String)>,
) -> ApiResult<Json<Value>> {
    let task = owned_task(&s, &slug, &id).await?;
    require_webhook(&task)?;
    let got = routines(&s)?
        .get_delivery(&id, &delivery_id)
        .await
        .map_err(delivery_miss)?;
    Ok(Json(crate::chat::routines::inspect::delivery_detail(
        &task, &got,
    )))
}

pub(crate) async fn bot_routine_enabled(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let id = body["id"].as_str().unwrap_or("");
    owned_routine(&s, &slug, id).await?;
    Ok(Json(
        crate::chat::routines::change(
            routines(&s)?,
            &s.routine_history,
            "enabled",
            json!({ "id": id, "enabled": body["enabled"] == true }),
        )
        .await?,
    ))
}

pub(crate) async fn bot_routine_delete(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let id = parse(&body)?["id"].as_str().unwrap_or("").to_string();
    owned_routine(&s, &slug, &id).await?;
    Ok(Json(
        crate::chat::routines::change(
            routines(&s)?,
            &s.routine_history,
            "delete",
            json!({ "id": id }),
        )
        .await?,
    ))
}

pub(crate) async fn bot_routine_run(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let id = parse(&body)?["id"].as_str().unwrap_or("").to_string();
    owned_routine(&s, &slug, &id).await?;
    Ok(Json(
        crate::chat::routines::change(
            routines(&s)?,
            &s.routine_history,
            "run",
            json!({ "id": id }),
        )
        .await?,
    ))
}
