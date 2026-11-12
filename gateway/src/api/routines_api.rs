use super::*;

pub(crate) async fn routines_list(State(s): S) -> ApiResult<Json<Value>> {
    let mut listed = routines::list(&s.t3).await?;
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
        routines::change(
            &s.t3,
            &s.routine_history,
            "upsert",
            routines::UPSERT,
            parse(&body)?,
        )
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
        routines::change(
            &s.t3,
            &s.routine_history,
            "enabled",
            routines::SET_ENABLED,
            parse(&body)?,
        )
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
        routines::change(
            &s.t3,
            &s.routine_history,
            "delete",
            routines::DELETE,
            parse(&body)?,
        )
        .await?,
    ))
}

pub(crate) async fn routines_run(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    Ok(Json(
        routines::change(
            &s.t3,
            &s.routine_history,
            "run",
            routines::RUN_NOW,
            parse(&body)?,
        )
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
    let listed = routines::list(&s.t3)
        .await
        .map_err(|e| ApiError::new(502, format!("T3 didn't list routines: {}", e.message)))?;
    let tasks: Vec<Value> = routines::tasks_for_thread(&listed, thread)
        .iter()
        .map(routines::task_row)
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
    let task = routines::new_routine(
        &bot,
        body["title"].as_str().unwrap_or(""),
        body["kind"].as_str().unwrap_or("hour"),
        unix_now(),
    )
    .map_err(ApiError::bad_request)?;
    Ok(Json(
        routines::change(&s.t3, &s.routine_history, "upsert", routines::UPSERT, task).await?,
    ))
}

pub(crate) async fn owned_routine(s: &AppState, slug: &str, id: &str) -> ApiResult<()> {
    let bot = require_bot(s, slug).await?;
    let thread = bot["mainThreadId"].as_str().unwrap_or("");
    let listed = routines::list(&s.t3).await?;
    if routines::tasks_for_thread(&listed, thread)
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
        routines::change(
            &s.t3,
            &s.routine_history,
            "enabled",
            routines::SET_ENABLED,
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
        routines::change(
            &s.t3,
            &s.routine_history,
            "delete",
            routines::DELETE,
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
        routines::change(
            &s.t3,
            &s.routine_history,
            "run",
            routines::RUN_NOW,
            json!({ "id": id }),
        )
        .await?,
    ))
}
