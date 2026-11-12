use super::*;

/// The folder this process should use. Environment, then `storage.json`, then the default.
/// A saved folder that fails the folder rules yields the default instead of an error.
pub(crate) fn resolve_root(data_root: &Path, env: &ProcessEnv) -> PathBuf {
    if let Some(root) = &env.root {
        return expand_tilde(root);
    }
    let load = read_load(data_root);
    if load.error.is_none() {
        if let Some(root) = &load.stored.root {
            if let Ok(path) = validate_folder(data_root, root) {
                return path;
            }
        }
    }
    default_root(data_root)
}

pub(crate) fn root_matches_env(env: &ProcessEnv, requested: Option<&str>) -> bool {
    let Some(locked) = &env.root else {
        return true;
    };
    let Some(requested) = requested else {
        return false;
    };
    same_path(&expand_tilde(locked), &expand_tilde(requested))
}

/// Copy the running Bots folder. The source is never deleted, including when the copy fails.
pub(crate) fn copy_folder(source: &Path, data_root: &Path, raw_dest: &str) -> ApiResult<Value> {
    let dest = validate_folder(data_root, raw_dest).map_err(ApiError::bad_request)?;
    if same_path(source, &dest) {
        return Err(ApiError::bad_request("Choose a different folder."));
    }
    if dest.starts_with(source) || source.starts_with(&dest) {
        return Err(ApiError::bad_request(
            "The folder cannot be inside the current Bots folder.",
        ));
    }
    if dir_has_entries(&dest) {
        return Err(ApiError::bad_request(
            "The folder is not empty. Nothing was deleted.",
        ));
    }
    let files = match copy_tree(source, &dest) {
        Ok(files) => files,
        Err(error) => return Err(ApiError::from(error)),
    };
    let source_hashes = tree_hashes(source);
    let dest_hashes = tree_hashes(&dest);
    if source_hashes != dest_hashes {
        return Err(ApiError::new(
            500,
            "The copy did not match. The original folder was left in place.",
        ));
    }
    Ok(json!({
        "ok": true,
        "root": dest.display().to_string(),
        "files": files,
        "anytype": anytype_status(),
    }))
}
