use super::*;

pub(crate) fn normalize_host(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Ok(DEFAULT_HOST.into());
    }
    let url = url::Url::parse(trimmed)
        .map_err(|_error| "The Anytype host must be a URL with no path.".to_string())?;
    let scheme_ok = matches!(url.scheme(), "http" | "https");
    let path_ok = url.path() == "/" || url.path().is_empty();
    if !scheme_ok
        || !path_ok
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
    {
        return Err("The Anytype host must be a URL with no path.".into());
    }
    let host = url.host_str().unwrap_or_default();
    let port = match url.port() {
        Some(port) => format!(":{port}"),
        None => String::new(),
    };
    Ok(format!("{}://{host}{port}", url.scheme()))
}

pub(crate) fn normalize_space(raw: &str) -> Result<String, String> {
    let space = raw.trim();
    if space.is_empty() || space.len() > 128 || space.chars().any(char::is_control) {
        return Err("The Anytype space name is not usable.".into());
    }
    Ok(space.to_string())
}

pub(crate) fn effective_host(env: &ProcessEnv, stored: &Stored) -> String {
    env.anytype_url
        .as_deref()
        .and_then(|value| normalize_host(value).ok())
        .unwrap_or_else(|| stored.host.clone())
}

pub(crate) fn effective_space(env: &ProcessEnv, stored: &Stored) -> String {
    env.anytype_space
        .as_deref()
        .and_then(|value| normalize_space(value).ok())
        .unwrap_or_else(|| stored.space.clone())
}

pub(crate) fn key_is_set(data_root: &Path, env: &ProcessEnv) -> bool {
    if env.key_locked {
        return env.key_set;
    }
    file_nonempty(&key_path(data_root))
}

pub(crate) async fn write_key(data_root: &Path, env: &ProcessEnv, key: &str) -> ApiResult<Value> {
    if env.key_locked {
        return Err(ApiError::new(
            409,
            "The Anytype key is set by the environment.",
        ));
    }
    let key = key.trim();
    if key.is_empty() || key.len() > 4096 {
        return Err(ApiError::bad_request("The Anytype key is not usable."));
    }
    crate::storage::write_atomic(&key_path(data_root), key.as_bytes(), Some(0o600))
        .await
        .map_err(ApiError::from)?;
    Ok(json!({ "keySet": true }))
}

pub(crate) async fn remove_key(data_root: &Path, env: &ProcessEnv) -> ApiResult<Value> {
    if env.key_locked {
        return Err(ApiError::new(
            409,
            "The Anytype key is set by the environment.",
        ));
    }
    match tokio::fs::remove_file(key_path(data_root)).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(ApiError::from(error)),
    }
    Ok(json!({ "keySet": false }))
}

pub(crate) async fn put_storage_key(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    surface: Option<Extension<Surface>>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    ensure_local(peer, &headers, surface_of(surface).as_ref())?;
    let body = parse_json(&headers, &body)?;
    let key = match body.get("key") {
        Some(Value::String(text)) => text.clone(),
        _ => return Err(ApiError::bad_request("The Anytype key is not usable.")),
    };
    Ok(Json(
        write_key(&state.cfg.data_root, &state.cfg.storage, &key).await?,
    ))
}

pub(crate) fn read_key(data_root: &Path, env: &ProcessEnv) -> Option<String> {
    let text = if env.key_locked {
        match (
            non_empty_var("HBOTS_ANYTYPE_KEY"),
            non_empty_var("HBOTS_ANYTYPE_KEY_FILE"),
        ) {
            (Some(key), _) => key,
            (None, Some(path)) => std::fs::read_to_string(path).ok()?,
            _ => return None,
        }
    } else {
        std::fs::read_to_string(key_path(data_root)).ok()?
    };
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Starts pairing with the saved host. Anytype Desktop shows the approval dialog.
pub(crate) async fn pair_start(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    surface: Option<Extension<Surface>>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    ensure_local(peer, &headers, surface_of(surface).as_ref())?;
    if state.cfg.storage.key_locked {
        return Err(ApiError::new(
            409,
            "The Anytype key is set by the environment.",
        ));
    }
    let load = read_load(&state.cfg.data_root);
    let host = effective_host(&state.cfg.storage, &load.stored);
    let challenge = crate::storage::adapters::anytype::start_pairing(&host).await?;
    Ok(Json(json!({ "challengeId": challenge, "host": host })))
}

/// Exchanges the 4 digit code for a scoped key and stores it. The key is never returned.
pub(crate) async fn pair_confirm(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    surface: Option<Extension<Surface>>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    ensure_local(peer, &headers, surface_of(surface).as_ref())?;
    let body = parse_json(&headers, &body)?;
    let challenge = field_string(&body, "challengeId")?
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or_else(|| ApiError::bad_request("The pairing challenge is missing."))?;
    let code = field_string(&body, "code")?
        .map(|value| value.trim().to_string())
        .filter(|value| value.len() == 4 && value.chars().all(|c| c.is_ascii_digit()))
        .ok_or_else(|| ApiError::bad_request("The code is 4 digits."))?;
    let load = read_load(&state.cfg.data_root);
    let host = effective_host(&state.cfg.storage, &load.stored);
    let (key, grant) =
        crate::storage::adapters::anytype::confirm_pairing(&host, &challenge, &code).await?;
    write_key(&state.cfg.data_root, &state.cfg.storage, &key).await?;
    Ok(Json(json!({
        "keySet": true,
        "permission": grant["permission"],
        "allSpaces": grant["all_spaces"],
    })))
}

/// Checks the saved key against Anytype: scope, reachable spaces, and whether the chosen
/// space is private.
pub(crate) async fn anytype_check(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    surface: Option<Extension<Surface>>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    ensure_local(peer, &headers, surface_of(surface).as_ref())?;
    let key = read_key(&state.cfg.data_root, &state.cfg.storage)
        .ok_or_else(|| ApiError::new(409, "Connect to Anytype first."))?;
    let load = read_load(&state.cfg.data_root);
    let host = effective_host(&state.cfg.storage, &load.stored);
    let space = effective_space(&state.cfg.storage, &load.stored);
    let mut value = crate::storage::adapters::anytype::status(&host, &key, &space).await?;
    value["schemaReady"] = json!(schema_ready(&host, &key, &space).await);
    Ok(Json(value))
}

async fn schema_ready(host: &str, key: &str, space: &str) -> bool {
    let Ok(client) = crate::storage::adapters::anytype::open_client(host, key, space).await else {
        return false;
    };
    crate::storage::adapters::anytype::schema_ready(&client)
        .await
        .unwrap_or(false)
}

/// Creates the hbots types in the saved space. `dryRun` lists what is missing.
pub(crate) async fn anytype_setup(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    surface: Option<Extension<Surface>>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    ensure_local(peer, &headers, surface_of(surface).as_ref())?;
    let body = parse_json(&headers, &body)?;
    let dry_run = match body.get("dryRun") {
        None => false,
        Some(Value::Bool(value)) => *value,
        Some(_) => return Err(ApiError::bad_request("dryRun must be true or false.")),
    };
    let key = read_key(&state.cfg.data_root, &state.cfg.storage)
        .ok_or_else(|| ApiError::new(409, "Connect to Anytype first."))?;
    let load = read_load(&state.cfg.data_root);
    let host = effective_host(&state.cfg.storage, &load.stored);
    let space = effective_space(&state.cfg.storage, &load.stored);
    let client = crate::storage::adapters::anytype::open_client(&host, &key, &space).await?;
    let report = crate::storage::adapters::anytype::bootstrap(&client, dry_run)
        .await
        .map_err(setup_error)?;
    Ok(Json(json!({
        "dryRun": report.dry_run,
        "types": report.types,
        "properties": report.properties,
        "meta": report.meta,
        "schemaReady": report.schema_ready,
        "keys": report.keys,
    })))
}

fn setup_error(error: crate::storage::adapters::anytype::AnytypeError) -> ApiError {
    use crate::storage::adapters::anytype::AnytypeError;
    match error {
        AnytypeError::Unreachable => ApiError::new(
            502,
            "Anytype is not reachable. Open the Anytype app and check the host.",
        ),
        AnytypeError::NotGranted => ApiError::new(403, "Anytype did not grant this space."),
        AnytypeError::RateLimited { .. } => ApiError::new(429, "Anytype is busy. Try again."),
        AnytypeError::EtagMismatch { .. }
        | AnytypeError::ObjectArchived
        | AnytypeError::Gone
        | AnytypeError::ApiChanged
        | AnytypeError::Bad { .. } => ApiError::new(502, "Anytype could not prepare the space."),
    }
}

pub(crate) async fn delete_storage_key(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    surface: Option<Extension<Surface>>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    ensure_local(peer, &headers, surface_of(surface).as_ref())?;
    Ok(Json(
        remove_key(&state.cfg.data_root, &state.cfg.storage).await?,
    ))
}
