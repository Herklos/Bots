use super::*;

pub(crate) fn ensure_local(
    peer: SocketAddr,
    headers: &HeaderMap,
    surface: Option<&Surface>,
) -> ApiResult<()> {
    let via_tunnel = [
        "cf-ray",
        "cf-connecting-ip",
        "x-forwarded-for",
        "cf-access-jwt-assertion",
    ]
    .iter()
    .any(|name| headers.contains_key(*name));
    if via_tunnel || surface == Some(&Surface::Tailnet) || !peer.ip().is_loopback() {
        return Err(ApiError::new(
            403,
            "Storage changes only accept local requests.",
        ));
    }
    Ok(())
}

pub(crate) fn require_json(headers: &HeaderMap) -> ApiResult<()> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if content_type
        .to_ascii_lowercase()
        .starts_with("application/json")
    {
        Ok(())
    } else {
        Err(ApiError::new(415, "content-type must be application/json"))
    }
}

pub(crate) fn parse_json(headers: &HeaderMap, body: &Bytes) -> ApiResult<Value> {
    require_json(headers)?;
    if body.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_slice(body).map_err(|_error| ApiError::bad_request("invalid JSON body"))
}

pub(crate) fn field_string(body: &Value, name: &str) -> ApiResult<Option<String>> {
    match body.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(ApiError::bad_request(format!("{name} must be a string."))),
    }
}

pub(crate) fn can_restart(home: &Path) -> bool {
    if std::env::var("HBOTS_SERVICE").ok().as_deref() != Some("1") {
        return false;
    }
    let os = std::env::consts::OS;
    crate::platform::service::supported(os)
        && crate::platform::service::installed(&crate::platform::cli::service_spec(home), os)
}

pub(crate) fn surface_of(surface: Option<Extension<Surface>>) -> Option<Surface> {
    surface.map(|Extension(value)| value)
}

pub(crate) async fn get_storage(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(view(
        &state.cfg.data_root,
        &state.cfg.bots_root,
        &state.cfg.storage,
        can_restart(&state.cfg.data_root),
    ))
}

pub(crate) async fn put_storage(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    surface: Option<Extension<Surface>>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    ensure_local(peer, &headers, surface_of(surface).as_ref())?;
    let body = parse_json(&headers, &body)?;
    let value = save(
        &state.cfg.data_root,
        &state.cfg.bots_root,
        &state.cfg.storage,
        can_restart(&state.cfg.data_root),
        &body,
    )
    .await?;
    Ok(Json(value))
}

pub(crate) async fn test_storage(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    surface: Option<Extension<Surface>>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    ensure_local(peer, &headers, surface_of(surface).as_ref())?;
    let body = parse_json(&headers, &body)?;
    Ok(Json(test_choice(&state.cfg.data_root, &body)?))
}

pub(crate) async fn copy_storage(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    surface: Option<Extension<Surface>>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    ensure_local(peer, &headers, surface_of(surface).as_ref())?;
    let body = parse_json(&headers, &body)?;
    let root = field_string(&body, "root")?
        .ok_or_else(|| ApiError::bad_request("The folder must be a string."))?;
    Ok(Json(copy_folder(
        &state.cfg.bots_root,
        &state.cfg.data_root,
        &root,
    )?))
}

pub(crate) async fn restart_storage(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    surface: Option<Extension<Surface>>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    ensure_local(peer, &headers, surface_of(surface).as_ref())?;
    if !can_restart(&state.cfg.data_root) {
        return Err(ApiError::new(409, "Restart hbots to apply this."));
    }
    let home = state.cfg.data_root.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let spec = crate::platform::cli::service_spec(&home);
        let os = std::env::consts::OS;
        match tokio::task::spawn_blocking(move || crate::platform::service::restart(&spec, os))
            .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::error!("restarting hbots: {error:#}"),
            Err(error) => tracing::error!("restarting hbots: {error}"),
        }
    });
    Ok(Json(json!({ "ok": true })))
}
