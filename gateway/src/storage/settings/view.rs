use super::*;

pub(crate) fn fallback_error(data_root: &Path, env: &ProcessEnv, load: &Load) -> Option<String> {
    if env.root.is_some() {
        return load.error.clone();
    }
    if let Some(root) = &load.stored.root {
        if validate_folder(data_root, root).is_err() {
            return load.error.clone();
        }
    }
    load.error.clone()
}

/// What `GET /api/storage` returns. `effective` is what the next start will use,
/// except the backend: `anytype` is stored and then replaced by the file system.
pub(crate) fn view(
    data_root: &Path,
    running_root: &Path,
    env: &ProcessEnv,
    can_restart: bool,
) -> Value {
    let load = read_load(data_root);
    let next_root = resolve_root(data_root, env);
    let error = fallback_error(data_root, env, &load).or_else(|| env_storage_error(env));
    json!({
        "effective": {
            "backend": Backend::Filesystem.as_str(),
            "root": next_root.display().to_string(),
            "host": effective_host(env, &load.stored),
            "space": effective_space(env, &load.stored),
            "keySet": key_is_set(data_root, env),
        },
        "saved": {
            "backend": load.stored.backend.as_str(),
            "root": load.stored.root,
            "host": load.stored.host,
            "space": load.stored.space,
            "keySet": file_nonempty(&key_path(data_root)),
        },
        "locked": {
            "backend": env.storage.is_some(),
            "root": env.root.is_some(),
            "host": env.anytype_url.is_some(),
            "space": env.anytype_space.is_some(),
            "key": env.key_locked,
        },
        "available": [Backend::Filesystem.as_str()],
        "restartRequired": !same_path(&next_root, running_root),
        "canRestart": can_restart,
        "anytype": anytype_status(),
        "error": error,
    })
}

pub(crate) fn env_storage_error(env: &ProcessEnv) -> Option<String> {
    let value = env.storage.as_deref()?;
    if Backend::parse(value).is_err() {
        return Some(
            "HBOTS_STORAGE must be filesystem or anytype. hbots is using the file system.".into(),
        );
    }
    if let Some(host) = &env.anytype_url {
        if normalize_host(host).is_err() {
            return Some(
                "HBOTS_ANYTYPE_URL must be a URL with no path. hbots is keeping the saved host."
                    .into(),
            );
        }
    }
    None
}
