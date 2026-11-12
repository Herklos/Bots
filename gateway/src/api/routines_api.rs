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
    Ok(Json(
        crate::chat::routines::change(routines(&s)?, &s.routine_history, "upsert", parse(&body)?)
            .await?,
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
    let listed = crate::chat::routines::list(routines(&s)?)
        .await
        .map_err(|e| ApiError::new(502, format!("T3 didn't list routines: {}", e.message)))?;
    let tasks: Vec<Value> = crate::chat::routines::tasks_for_thread(&listed, thread)
        .iter()
        .map(crate::chat::routines::task_row)
        .collect();
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
    let task = crate::chat::routines::new_routine(
        &bot,
        body["title"].as_str().unwrap_or(""),
        body["kind"].as_str().unwrap_or("hour"),
        unix_now(),
    )
    .map_err(ApiError::bad_request)?;
    Ok(Json(
        crate::chat::routines::change(routines(&s)?, &s.routine_history, "upsert", task).await?,
    ))
}

pub(crate) async fn owned_routine(s: &AppState, slug: &str, id: &str) -> ApiResult<()> {
    let bot = require_bot(s, slug).await?;
    let thread = bot["mainThreadId"].as_str().unwrap_or("");
    let listed = crate::chat::routines::list(routines(s)?).await?;
    if crate::chat::routines::tasks_for_thread(&listed, thread)
        .iter()
        .any(|t| t["id"] == id)
    {
        Ok(())
    } else {
        Err(ApiError::not_found("That routine is not on this Bot."))
    }
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
