//! The sync API: status, run, the first import, and conflict resolution. Local only,
//! the same as the other storage routes: a session still cannot drive Anytype sync
//! through remote access.

use super::*;

/// `GET /api/sync/status`. The live snapshot, or why the engine is off.
pub(crate) async fn get_sync_status(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    surface: Option<Extension<Surface>>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    ensure_local(peer, &headers, surface_of(surface).as_ref())?;
    Ok(Json(status_of(&state)))
}

/// `POST /api/sync/run`. One cycle now. The answer is the snapshot; the cycle lands
/// within the next tick, so the first call may still show the previous state.
pub(crate) async fn post_sync_run(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    surface: Option<Extension<Surface>>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    ensure_local(peer, &headers, surface_of(surface).as_ref())?;
    let handle = sync_handle(&state)?;
    if let Some(runtime) = handle.runtime() {
        runtime.run_now();
    }
    Ok(Json(handle.status()))
}

/// `POST /api/sync/import`. `dryRun: true` answers the preview; `dryRun: false`
/// starts the first import. Nothing is pushed before the confirm.
pub(crate) async fn post_sync_import(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    surface: Option<Extension<Surface>>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    ensure_local(peer, &headers, surface_of(surface).as_ref())?;
    let body = parse_json(&headers, &body)?;
    let dry_run = match body.get("dryRun") {
        None | Some(Value::Null) => true,
        Some(Value::Bool(value)) => *value,
        Some(_) => return Err(ApiError::bad_request("dryRun must be true or false.")),
    };
    let handle = sync_handle(&state)?;
    if dry_run {
        if let Some(runtime) = handle.runtime() {
            let preview = runtime
                .dry_run()
                .await
                .map_err(|reason| ApiError::new(502, format!("The dry run failed. {reason}")))?;
            return Ok(Json(json!({ "dryRun": true, "preview": preview })));
        }
        return Err(ApiError::new(
            409,
            handle.reason().unwrap_or("The sync engine is off."),
        ));
    }
    if let Some(runtime) = handle.runtime() {
        runtime.start_import();
    }
    let mut status = handle.status();
    status["started"] = json!(true);
    Ok(Json(status))
}

/// `GET /api/sync/conflicts`. The open list, local record first.
pub(crate) async fn get_sync_conflicts(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    surface: Option<Extension<Surface>>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    ensure_local(peer, &headers, surface_of(surface).as_ref())?;
    let handle = state.sync.get();
    let conflicts = match handle
        .as_ref()
        .filter(|handle| handle.running())
        .and_then(|handle| handle.runtime())
    {
        Some(runtime) => runtime
            .snapshot()
            .conflicts
            .iter()
            .map(|(id, path)| json!({ "id": id, "path": path }))
            .collect::<Vec<Value>>(),
        None => Vec::new(),
    };
    Ok(Json(json!({ "conflicts": conflicts })))
}

/// `POST /api/sync/conflicts/{id}/resolve`. Keep mine, Keep Anytype, or Keep both.
pub(crate) async fn post_sync_conflict_resolve(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    surface: Option<Extension<Surface>>,
    headers: HeaderMap,
    path: axum::extract::Path<String>,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    ensure_local(peer, &headers, surface_of(surface).as_ref())?;
    let body = parse_json(&headers, &body)?;
    let keep = match body.get("keep").and_then(Value::as_str) {
        Some("local") | Some("mine") => crate::storage::sync::Keep::Local,
        Some("remote") | Some("anytype") => crate::storage::sync::Keep::Remote,
        Some("both") => crate::storage::sync::Keep::Both,
        _ => {
            return Err(ApiError::bad_request(
                "keep must be local, remote, or both.",
            ))
        }
    };
    let handle = sync_handle(&state)?;
    let runtime = handle
        .runtime()
        .ok_or_else(|| ApiError::new(409, "The sync engine is not running."))?;
    runtime
        .resolve(&path, keep)
        .await
        .map_err(|reason| ApiError::new(502, format!("The conflict was not resolved. {reason}")))?;
    Ok(Json(handle.status()))
}

fn sync_handle(state: &AppState) -> ApiResult<Arc<crate::storage::sync::SyncHandle>> {
    state
        .sync
        .get()
        .ok_or_else(|| ApiError::new(409, "Storage is the file system."))
}

fn status_of(state: &AppState) -> Value {
    match state.sync.get() {
        Some(handle) => handle.status(),
        None => json!({
            "available": false,
            "running": false,
            "state": "filesystem",
            "reason": "Storage is the file system.",
        }),
    }
}
