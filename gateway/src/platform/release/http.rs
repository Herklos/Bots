use super::*;

// ---------- routes ----------

pub(crate) async fn system_status(
    State(s): State<Arc<AppState>>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let update = s
        .system
        .check(q.get("check").is_some_and(|v| v == "1"))
        .await;
    let auto = s.system.auto_status().await;
    let mut out = json!({
        "version": VERSION,
        "installed": installed_runtime(&std::env::current_exe().unwrap_or_default(), &s.cfg.data_root),
        "service": std::env::var("HBOTS_SERVICE").is_ok_and(|v| v == "1"),
    });
    if let (Value::Object(base), Value::Object(update), Value::Object(auto)) = (
        &mut out,
        serde_json::to_value(&update)
            .map_err(|e| crate::platform::error::ApiError::new(500, e.to_string()))?,
        auto,
    ) {
        base.extend(update);
        base.extend(auto);
    }
    Ok(Json(out))
}

pub(crate) async fn set_auto_update(State(s): State<Arc<AppState>>, body: Bytes) -> Response {
    let auto = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|v| v["auto"].as_bool());
    let Some(auto) = auto else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "auto must be true or false" })),
        )
            .into_response();
    };
    let mut state = s.system.auto.lock().await;
    state.auto = auto;
    state.waiting = None;
    state.gave_up = None;
    s.system.persist(&state);
    drop(state);
    Json(s.system.auto_status().await).into_response()
}

#[derive(Deserialize)]
pub(crate) struct NoticeAt {
    pub(super) at: Option<String>,
}

pub(crate) async fn dismiss_notice(
    State(s): State<Arc<AppState>>,
    Query(q): Query<NoticeAt>,
) -> Json<Value> {
    let mut state = s.system.auto.lock().await;
    if state
        .notice
        .as_ref()
        .is_some_and(|n| q.at.as_ref().is_none_or(|at| *at == n.at))
    {
        state.notice = None;
        s.system.persist(&state);
    }
    drop(state);
    Json(s.system.auto_status().await)
}

pub(crate) async fn start_update(State(s): State<Arc<AppState>>) -> ApiResult<Response> {
    let exe = std::env::current_exe().unwrap_or_default();
    if !installed_runtime(&exe, &s.cfg.data_root) {
        return Err(crate::platform::error::ApiError::new(
            409,
            "This copy runs from a source checkout. Install a release to update it.",
        ));
    }
    let status = s.system.check(true).await;
    let Some(latest) = status.latest.filter(|_| status.update_available) else {
        return Err(crate::platform::error::ApiError::new(
            409,
            format!("hbots {VERSION} is up to date."),
        ));
    };
    let preview = is_preview(VERSION);
    start_updater(&latest.version, preview, &s.cfg.data_root)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "started": true, "version": latest.version })),
    )
        .into_response())
}
