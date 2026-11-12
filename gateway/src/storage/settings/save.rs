use super::*;

pub(crate) async fn write_stored(data_root: &Path, stored: &Stored) -> ApiResult<()> {
    let text =
        serde_json::to_string_pretty(&stored.to_json()).expect("storage JSON serializes") + "\n";
    crate::storage::write_atomic(&storage_path(data_root), text.as_bytes(), None)
        .await
        .map_err(ApiError::from)
}

/// Save backend, folder, host, and space. Locked fields reject a different value.
/// The key is never read from this body.
pub(crate) async fn save(
    data_root: &Path,
    running_root: &Path,
    env: &ProcessEnv,
    can_restart: bool,
    body: &Value,
) -> ApiResult<Value> {
    let mut load = read_load(data_root);
    if body.get("backend").is_some() {
        let requested = field_string(body, "backend")?
            .ok_or_else(|| ApiError::bad_request("Storage must be filesystem or anytype."))?;
        let backend = Backend::parse(&requested).map_err(ApiError::bad_request)?;
        if env.storage.is_some() && env.storage.as_deref() != Some(backend.as_str()) {
            return Err(ApiError::new(409, "Storage is set by the environment."));
        }
        load.stored.backend = backend;
    }
    if body
        .as_object()
        .is_some_and(|object| object.contains_key("root"))
    {
        let requested = match &body["root"] {
            Value::Null => None,
            Value::String(text) => Some(text.clone()),
            _ => {
                return Err(ApiError::bad_request(
                    "The folder must be a string or null.",
                ))
            }
        };
        if env.root.is_some() && !root_matches_env(env, requested.as_deref()) {
            return Err(ApiError::new(409, "The folder is set by the environment."));
        }
        if let Some(root) = &requested {
            let path = validate_folder(data_root, root).map_err(ApiError::bad_request)?;
            load.stored.root = Some(path.display().to_string());
        } else {
            load.stored.root = None;
        }
    }
    if body.get("host").is_some() {
        let requested = field_string(body, "host")?
            .ok_or_else(|| ApiError::bad_request("The Anytype host must be a URL with no path."))?;
        let host = normalize_host(&requested).map_err(ApiError::bad_request)?;
        if env.anytype_url.is_some()
            && normalize_host(env.anytype_url.as_deref().unwrap_or_default())
                .ok()
                .as_deref()
                != Some(host.as_str())
        {
            return Err(ApiError::new(
                409,
                "The Anytype host is set by the environment.",
            ));
        }
        load.stored.host = host;
    }
    if body.get("space").is_some() {
        let requested = field_string(body, "space")?
            .ok_or_else(|| ApiError::bad_request("The Anytype space name is not usable."))?;
        let space = normalize_space(&requested).map_err(ApiError::bad_request)?;
        if env.anytype_space.is_some() && env.anytype_space.as_deref() != Some(space.as_str()) {
            return Err(ApiError::new(
                409,
                "The Anytype space is set by the environment.",
            ));
        }
        load.stored.space = space;
    }
    load.stored.key_name = KEY_NAME.into();
    write_stored(data_root, &load.stored).await?;
    Ok(view(data_root, running_root, env, can_restart))
}

/// Check a folder, host, backend, or space without writing `storage.json`.
pub(crate) fn test_choice(data_root: &Path, body: &Value) -> ApiResult<Value> {
    let backend = match field_string(body, "backend")? {
        Some(value) => Backend::parse(&value).map_err(ApiError::bad_request)?,
        None => Backend::Filesystem,
    };
    let root = if body
        .as_object()
        .is_some_and(|object| object.contains_key("root"))
        && !body["root"].is_null()
    {
        let text = field_string(body, "root")?
            .ok_or_else(|| ApiError::bad_request("The folder must be a string or null."))?;
        Some(
            validate_folder(data_root, &text)
                .map_err(ApiError::bad_request)?
                .display()
                .to_string(),
        )
    } else {
        None
    };
    let host = match field_string(body, "host")? {
        Some(value) => normalize_host(&value).map_err(ApiError::bad_request)?,
        None => DEFAULT_HOST.into(),
    };
    let space = match field_string(body, "space")? {
        Some(value) => normalize_space(&value).map_err(ApiError::bad_request)?,
        None => DEFAULT_SPACE.into(),
    };
    Ok(json!({
        "ok": true,
        "backend": backend.as_str(),
        "root": root,
        "host": host,
        "space": space,
        "anytype": anytype_status(),
    }))
}
