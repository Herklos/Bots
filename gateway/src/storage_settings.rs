//! Where Bots are stored. The running gateway uses the file system. A saved
//! `anytype` choice is kept and reported unavailable.
//!
//! `~/.hbots/storage.json` (the hbots home, outside the Bots root) holds the choice.
//! Environment variables win and lock the field they set, the same way
//! `HBOTS_NETWORK_ACCESS` locks network access. A bad saved folder does not stop
//! startup: the process uses the file-system default and `GET /api/storage` says why.
//! Changing the folder does not move data. Copying checks hashes and leaves the
//! old folder in place. Responses never include an Anytype key.

use crate::error::{ApiError, ApiResult};
use crate::exposure::Surface;
use crate::AppState;
use axum::body::Bytes;
use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap};
use axum::Extension;
use axum::Json;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const DEFAULT_HOST: &str = "http://127.0.0.1:31009";
const DEFAULT_SPACE: &str = "hbots";
const KEY_NAME: &str = "hbots";
const FILE_NAME: &str = "storage.json";
const KEY_FILE: &str = "anytype-key";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessEnv {
    pub storage: Option<String>,
    pub root: Option<String>,
    pub anytype_url: Option<String>,
    pub anytype_space: Option<String>,
    /// `HBOTS_ANYTYPE_KEY` or `HBOTS_ANYTYPE_KEY_FILE` is set. The key itself is not kept.
    pub key_locked: bool,
    /// The environment supplies a non-empty key. The text is not kept.
    pub key_set: bool,
}

impl ProcessEnv {
    pub fn capture() -> Self {
        let key_var = non_empty_var("HBOTS_ANYTYPE_KEY");
        let key_file = non_empty_var("HBOTS_ANYTYPE_KEY_FILE");
        let key_set = key_var
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty())
            || key_file
                .as_deref()
                .is_some_and(|path| file_nonempty(Path::new(path)));
        Self {
            storage: non_empty_var("HBOTS_STORAGE"),
            root: non_empty_var("HBOTS_ROOT"),
            anytype_url: non_empty_var("HBOTS_ANYTYPE_URL"),
            anytype_space: non_empty_var("HBOTS_ANYTYPE_SPACE"),
            key_locked: key_var.is_some() || key_file.is_some(),
            key_set,
        }
    }
}

fn non_empty_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Backend {
    Filesystem,
    Anytype,
}

impl Backend {
    fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "filesystem" => Ok(Self::Filesystem),
            "anytype" => Ok(Self::Anytype),
            _ => Err("Storage must be filesystem or anytype.".into()),
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Filesystem => "filesystem",
            Self::Anytype => "anytype",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stored {
    backend: Backend,
    /// `None` is the default folder, `data_root/bots`.
    root: Option<String>,
    host: String,
    space: String,
    key_name: String,
}

impl Stored {
    fn defaults() -> Self {
        Self {
            backend: Backend::Filesystem,
            root: None,
            host: DEFAULT_HOST.into(),
            space: DEFAULT_SPACE.into(),
            key_name: KEY_NAME.into(),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "backend": self.backend.as_str(),
            "root": self.root,
            "anytype": {
                "host": self.host,
                "space": self.space,
                "keyName": self.key_name,
            }
        })
    }
}

struct Load {
    stored: Stored,
    /// Why a saved file was not applied. Startup still continues.
    error: Option<String>,
}

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

fn default_root(data_root: &Path) -> PathBuf {
    data_root.join("bots")
}

fn storage_path(data_root: &Path) -> PathBuf {
    data_root.join(FILE_NAME)
}

fn key_path(data_root: &Path) -> PathBuf {
    data_root.join(KEY_FILE)
}

fn read_load(data_root: &Path) -> Load {
    let path = storage_path(data_root);
    if !path.exists() {
        return Load {
            stored: Stored::defaults(),
            error: None,
        };
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => {
            return Load {
                stored: Stored::defaults(),
                error: Some(
                    "The saved storage settings could not be read. hbots is using the file system."
                        .into(),
                ),
            };
        }
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => {
            return Load {
                stored: Stored::defaults(),
                error: Some(
                    "The saved storage settings could not be read. hbots is using the file system."
                        .into(),
                ),
            };
        }
    };
    match stored_from_value(&value) {
        Ok(stored) => explain_stored(data_root, stored),
        Err(error) => Load {
            stored: Stored::defaults(),
            error: Some(error),
        },
    }
}

fn explain_stored(data_root: &Path, mut stored: Stored) -> Load {
    let mut error = None;
    if let Err(message) = Backend::parse(stored.backend.as_str()) {
        error = Some(message);
        stored.backend = Backend::Filesystem;
    }
    if let Some(root) = stored.root.clone() {
        if let Err(message) = validate_folder(data_root, &root) {
            error = Some(message);
        }
    }
    if let Err(message) = normalize_host(&stored.host) {
        error = Some(message);
        stored.host = DEFAULT_HOST.into();
    } else if let Ok(host) = normalize_host(&stored.host) {
        stored.host = host;
    }
    if let Err(message) = normalize_space(&stored.space) {
        error = Some(message);
        stored.space = DEFAULT_SPACE.into();
    }
    Load { stored, error }
}

fn stored_from_value(value: &Value) -> Result<Stored, String> {
    let Some(object) = value.as_object() else {
        return Err(
            "The saved storage settings could not be read. hbots is using the file system.".into(),
        );
    };
    let backend = match object.get("backend") {
        None => Backend::Filesystem,
        Some(Value::String(text)) => Backend::parse(text)?,
        Some(_) => {
            return Err(
                "The saved storage settings could not be read. hbots is using the file system."
                    .into(),
            );
        }
    };
    let root = match object.get("root") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(_) => {
            return Err("The saved folder is not a path. hbots is using the file system.".into());
        }
    };
    let anytype = object.get("anytype").cloned().unwrap_or_else(|| json!({}));
    let host = anytype["host"].as_str().unwrap_or(DEFAULT_HOST).to_string();
    let space = anytype["space"]
        .as_str()
        .unwrap_or(DEFAULT_SPACE)
        .to_string();
    let key_name = anytype["keyName"].as_str().unwrap_or(KEY_NAME).to_string();
    Ok(Stored {
        backend,
        root,
        host,
        space,
        key_name,
    })
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn expand_tilde(raw: &str) -> PathBuf {
    let raw = raw.trim();
    if raw == "~" {
        return home_dir();
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(raw)
}

fn validate_folder(data_root: &Path, raw: &str) -> Result<PathBuf, String> {
    let expanded = expand_tilde(raw);
    if !expanded.is_absolute() {
        return Err("The folder must be an absolute path.".into());
    }
    let path = canonical_candidate(&expanded)?;
    if same_path(&path, data_root) {
        return Err("The folder cannot be the hbots home.".into());
    }
    if path.components().any(|component| {
        let name = component.as_os_str();
        name == ".botapp" || name == ".bot"
    }) {
        return Err("The folder cannot be inside a Bot folder or .botapp.".into());
    }
    if inside_bot_folder(&path) {
        return Err("The folder cannot be inside a Bot folder or .botapp.".into());
    }
    if path.is_file() {
        return Err("The folder is a file.".into());
    }
    std::fs::create_dir_all(&path).map_err(|_error| "The folder cannot be created.".to_string())?;
    let probe = path.join(".hbots-write-probe");
    std::fs::write(&probe, b"ok").map_err(|_error| "The folder is not writable.".to_string())?;
    let _ = std::fs::remove_file(&probe);
    Ok(path)
}

fn canonical_candidate(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return std::fs::canonicalize(path)
            .map_err(|_error| "The folder cannot be created.".to_string());
    }
    let mut suffix = Vec::new();
    let mut cursor = path;
    loop {
        if cursor.exists() {
            let mut out = std::fs::canonicalize(cursor)
                .map_err(|_error| "The folder cannot be created.".to_string())?;
            for part in suffix.iter().rev() {
                out.push(part);
            }
            return Ok(out);
        }
        match cursor.file_name() {
            Some(name) => suffix.push(name.to_os_string()),
            None => return Err("The folder must be an absolute path.".into()),
        }
        match cursor.parent() {
            Some(parent) if parent != cursor => cursor = parent,
            _ => return Err("The folder cannot be created.".into()),
        }
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    let left = std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    left == right
}

fn inside_bot_folder(path: &Path) -> bool {
    let mut cursor = Some(path);
    while let Some(dir) = cursor {
        if dir.join(".bot").join("bot.json").is_file() {
            return true;
        }
        cursor = dir.parent().filter(|parent| *parent != dir);
    }
    false
}

fn normalize_host(raw: &str) -> Result<String, String> {
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

fn normalize_space(raw: &str) -> Result<String, String> {
    let space = raw.trim();
    if space.is_empty() || space.len() > 128 || space.chars().any(char::is_control) {
        return Err("The Anytype space name is not usable.".into());
    }
    Ok(space.to_string())
}

fn file_nonempty(path: &Path) -> bool {
    std::fs::metadata(path)
        .ok()
        .is_some_and(|meta| meta.is_file() && meta.len() > 0)
}

fn effective_host(env: &ProcessEnv, stored: &Stored) -> String {
    env.anytype_url
        .as_deref()
        .and_then(|value| normalize_host(value).ok())
        .unwrap_or_else(|| stored.host.clone())
}

fn effective_space(env: &ProcessEnv, stored: &Stored) -> String {
    env.anytype_space
        .as_deref()
        .and_then(|value| normalize_space(value).ok())
        .unwrap_or_else(|| stored.space.clone())
}

fn key_is_set(data_root: &Path, env: &ProcessEnv) -> bool {
    if env.key_locked {
        return env.key_set;
    }
    file_nonempty(&key_path(data_root))
}

fn fallback_error(data_root: &Path, env: &ProcessEnv, load: &Load) -> Option<String> {
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

fn env_storage_error(env: &ProcessEnv) -> Option<String> {
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

fn anytype_status() -> Value {
    json!({ "available": false, "status": "unavailable" })
}

fn ensure_local(peer: SocketAddr, headers: &HeaderMap, surface: Option<&Surface>) -> ApiResult<()> {
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

fn require_json(headers: &HeaderMap) -> ApiResult<()> {
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

fn parse_json(headers: &HeaderMap, body: &Bytes) -> ApiResult<Value> {
    require_json(headers)?;
    if body.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_slice(body).map_err(|_error| ApiError::bad_request("invalid JSON body"))
}

fn field_string(body: &Value, name: &str) -> ApiResult<Option<String>> {
    match body.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(ApiError::bad_request(format!("{name} must be a string."))),
    }
}

async fn write_stored(data_root: &Path, stored: &Stored) -> ApiResult<()> {
    let text =
        serde_json::to_string_pretty(&stored.to_json()).expect("storage JSON serializes") + "\n";
    crate::store::write_atomic(&storage_path(data_root), text.as_bytes(), None)
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

fn root_matches_env(env: &ProcessEnv, requested: Option<&str>) -> bool {
    let Some(locked) = &env.root else {
        return true;
    };
    let Some(requested) = requested else {
        return false;
    };
    same_path(&expand_tilde(locked), &expand_tilde(requested))
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

fn dir_has_entries(path: &Path) -> bool {
    std::fs::read_dir(path)
        .ok()
        .and_then(|mut entries| entries.next())
        .is_some()
}

fn copy_tree(source: &Path, dest: &Path) -> std::io::Result<u64> {
    std::fs::create_dir_all(dest)?;
    if !source.exists() {
        return Ok(0);
    }
    copy_children(source, dest)
}

fn copy_children(source: &Path, dest: &Path) -> std::io::Result<u64> {
    let mut files = 0_u64;
    if !source.exists() {
        return Ok(0);
    }
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let from = entry.path();
        let to = dest.join(&name);
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            let target = std::fs::read_link(&from)?;
            symlink_at(&target, &to)?;
        } else if file_type.is_dir() {
            std::fs::create_dir_all(&to)?;
            files += copy_children(&from, &to)?;
        } else if file_type.is_file() {
            std::fs::copy(&from, &to)?;
            files += 1;
        }
    }
    Ok(files)
}

fn symlink_at(target: &Path, dest: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, dest)
    }
    #[cfg(not(unix))]
    {
        let _ = (target, dest);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "symlinks are not copied on this system",
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EntryHash {
    File(String),
    Link(PathBuf),
}

fn tree_hashes(root: &Path) -> BTreeMap<String, EntryHash> {
    let mut out = BTreeMap::new();
    if root.is_dir() {
        hash_dir(root, "", &mut out);
    }
    out
}

fn hash_dir(dir: &Path, prefix: &str, out: &mut BTreeMap<String, EntryHash>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let rel = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            if let Ok(target) = std::fs::read_link(&path) {
                out.insert(rel, EntryHash::Link(target));
            }
        } else if file_type.is_dir() {
            hash_dir(&path, &rel, out);
        } else if file_type.is_file() {
            if let Ok(bytes) = std::fs::read(&path) {
                let mut hasher = Sha256::new();
                hasher.update(&bytes);
                out.insert(rel, EntryHash::File(hex::encode(hasher.finalize())));
            }
        }
    }
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
    crate::store::write_atomic(&key_path(data_root), key.as_bytes(), Some(0o600))
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

fn can_restart(home: &Path) -> bool {
    if std::env::var("HBOTS_SERVICE").ok().as_deref() != Some("1") {
        return false;
    }
    let os = std::env::consts::OS;
    crate::service::supported(os) && crate::service::installed(&crate::cli::service_spec(home), os)
}

fn surface_of(surface: Option<Extension<Surface>>) -> Option<Surface> {
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
        let spec = crate::cli::service_spec(&home);
        let os = std::env::consts::OS;
        match tokio::task::spawn_blocking(move || crate::service::restart(&spec, os)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::error!("restarting hbots: {error:#}"),
            Err(error) => tracing::error!("restarting hbots: {error}"),
        }
    });
    Ok(Json(json!({ "ok": true })))
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    fn env_root(path: &Path) -> ProcessEnv {
        ProcessEnv {
            root: Some(path.display().to_string()),
            ..ProcessEnv::default()
        }
    }

    fn write_file(home: &Path, stored: &Stored) {
        std::fs::create_dir_all(home).unwrap();
        std::fs::write(storage_path(home), stored.to_json().to_string()).unwrap();
    }

    #[test]
    fn environment_wins_over_the_file_and_the_file_over_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("hbots");
        let from_file = dir.path().join("from-file");
        let from_env = dir.path().join("from-env");
        std::fs::create_dir_all(&from_file).unwrap();
        std::fs::create_dir_all(&from_env).unwrap();
        std::fs::create_dir_all(home.join("bots")).unwrap();
        let mut stored = Stored::defaults();
        stored.root = Some(from_file.display().to_string());
        write_file(&home, &stored);

        let resolved = resolve_root(&home, &env_root(&from_env));
        assert_eq!(resolved, from_env);

        let resolved = resolve_root(&home, &ProcessEnv::default());
        assert_eq!(resolved, std::fs::canonicalize(&from_file).unwrap());

        std::fs::remove_file(storage_path(&home)).unwrap();
        let resolved = resolve_root(&home, &ProcessEnv::default());
        assert_eq!(resolved, home.join("bots"));
    }

    #[test]
    fn a_bad_saved_folder_falls_back_and_the_reason_is_on_the_view() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("hbots");
        std::fs::create_dir_all(home.join("bots")).unwrap();
        let mut stored = Stored::defaults();
        stored.root = Some(home.display().to_string());
        write_file(&home, &stored);
        let resolved = resolve_root(&home, &ProcessEnv::default());
        assert_eq!(resolved, home.join("bots"));
        let body = view(&home, &home.join("bots"), &ProcessEnv::default(), false);
        assert_eq!(body["effective"]["backend"], "filesystem");
        assert!(body["error"]
            .as_str()
            .unwrap()
            .contains("cannot be the hbots home"));
        assert_eq!(body["restartRequired"], false);
    }

    #[tokio::test]
    async fn a_locked_folder_rejects_a_different_value() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("hbots");
        let locked = dir.path().join("locked");
        let other = dir.path().join("other");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        let env = env_root(&locked);
        let error = save(
            &home,
            &locked,
            &env,
            false,
            &json!({ "root": other.display().to_string() }),
        )
        .await
        .unwrap_err();
        assert_eq!(error.status, StatusCode::CONFLICT);
        assert!(!storage_path(&home).exists());
    }

    #[test]
    fn folder_rules_reject_the_home_a_bot_folder_and_a_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("hbots");
        std::fs::create_dir_all(&home).unwrap();
        let bot = dir.path().join("library").join("ada");
        std::fs::create_dir_all(bot.join(".bot")).unwrap();
        std::fs::write(bot.join(".bot").join("bot.json"), b"{}").unwrap();
        let app = dir.path().join("tree").join(".botapp").join("inside");
        std::fs::create_dir_all(app.parent().unwrap()).unwrap();

        assert!(validate_folder(&home, home.to_str().unwrap())
            .unwrap_err()
            .contains("hbots home"));
        assert!(validate_folder(&home, bot.join("notes").to_str().unwrap())
            .unwrap_err()
            .contains(".botapp"));
        assert!(validate_folder(&home, app.to_str().unwrap())
            .unwrap_err()
            .contains(".botapp"));
        assert!(validate_folder(&home, "bots")
            .unwrap_err()
            .contains("absolute"));
    }

    #[test]
    fn copy_matches_hashes_and_leaves_the_old_folder() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("hbots");
        let source = home.join("bots");
        std::fs::create_dir_all(source.join("ada")).unwrap();
        std::fs::write(source.join("ada").join("note.txt"), b"keep-me").unwrap();
        let dest = dir.path().join("next");
        let body = copy_folder(&source, &home, dest.to_str().unwrap()).unwrap();
        assert_eq!(body["ok"], true);
        assert_eq!(body["files"], 1);
        assert_eq!(
            std::fs::read(source.join("ada").join("note.txt")).unwrap(),
            b"keep-me"
        );
        assert_eq!(
            std::fs::read(dest.join("ada").join("note.txt")).unwrap(),
            b"keep-me"
        );
        std::fs::remove_dir_all(&dest).unwrap();
        assert_eq!(
            std::fs::read(source.join("ada").join("note.txt")).unwrap(),
            b"keep-me"
        );
    }

    #[tokio::test]
    async fn saving_a_new_folder_asks_for_a_restart_and_does_not_move_files() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("hbots");
        let running = home.join("bots");
        std::fs::create_dir_all(running.join("ada")).unwrap();
        std::fs::write(running.join("ada").join("note.txt"), b"stay").unwrap();
        let next = dir.path().join("next");
        std::fs::create_dir_all(&home).unwrap();
        let before = view(&home, &running, &ProcessEnv::default(), false);
        assert_eq!(before["restartRequired"], false);
        let after = save(
            &home,
            &running,
            &ProcessEnv::default(),
            true,
            &json!({ "root": next.display().to_string() }),
        )
        .await
        .unwrap();
        assert_eq!(after["restartRequired"], true);
        assert_eq!(after["canRestart"], true);
        assert_eq!(
            after["effective"]["root"],
            std::fs::canonicalize(&next).unwrap().display().to_string()
        );
        assert_eq!(
            std::fs::read(running.join("ada").join("note.txt")).unwrap(),
            b"stay"
        );
        assert!(!next.join("ada").join("note.txt").exists());
    }

    #[tokio::test]
    async fn anytype_is_stored_and_reported_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("hbots");
        let running = home.join("bots");
        std::fs::create_dir_all(&running).unwrap();
        let body = save(
            &home,
            &running,
            &ProcessEnv::default(),
            false,
            &json!({
                "backend": "anytype",
                "host": "http://127.0.0.1:31009/",
                "space": "hbots"
            }),
        )
        .await
        .unwrap();
        assert_eq!(body["effective"]["backend"], "filesystem");
        assert_eq!(body["saved"]["backend"], "anytype");
        assert_eq!(body["saved"]["host"], DEFAULT_HOST);
        assert_eq!(body["available"], json!(["filesystem"]));
        assert_eq!(body["anytype"]["available"], false);
        assert_eq!(body["anytype"]["status"], "unavailable");
        let on_disk: Value =
            serde_json::from_str(&std::fs::read_to_string(storage_path(&home)).unwrap()).unwrap();
        assert_eq!(on_disk["backend"], "anytype");
        assert_eq!(on_disk["root"], Value::Null);
        assert_eq!(on_disk["anytype"]["keyName"], "hbots");
        let rejected =
            test_choice(&home, &json!({ "host": "http://127.0.0.1:31009/space" })).unwrap_err();
        assert_eq!(rejected.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn responses_say_whether_a_key_is_set_and_never_include_it() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("hbots");
        let running = home.join("bots");
        std::fs::create_dir_all(&running).unwrap();
        let secret = "super-secret-anytype-key-value";
        let written = write_key(&home, &ProcessEnv::default(), secret)
            .await
            .unwrap();
        assert_eq!(written, json!({ "keySet": true }));
        let saved = save(
            &home,
            &running,
            &ProcessEnv::default(),
            false,
            &json!({ "backend": "filesystem", "key": secret }),
        )
        .await
        .unwrap();
        let tested = test_choice(&home, &json!({ "backend": "filesystem" })).unwrap();
        let copied = {
            let dest = dir.path().join("copy");
            copy_folder(&running, &home, dest.to_str().unwrap()).unwrap()
        };
        let status = view(&home, &running, &ProcessEnv::default(), false);
        assert_eq!(status["effective"]["keySet"], true);
        assert_eq!(status["saved"]["keySet"], true);
        let removed = remove_key(&home, &ProcessEnv::default()).await.unwrap();
        assert_eq!(removed, json!({ "keySet": false }));
        assert!(!key_path(&home).exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            write_key(&home, &ProcessEnv::default(), secret)
                .await
                .unwrap();
            let mode = std::fs::metadata(key_path(&home))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        let blob = format!("{written}{saved}{tested}{copied}{status}{removed}");
        assert!(!blob.contains(secret));
        let disk = std::fs::read_to_string(storage_path(&home)).unwrap();
        assert!(!disk.contains(secret));
    }

    #[test]
    fn a_tilde_expands_and_a_trailing_host_slash_is_removed() {
        let home = home_dir();
        assert_eq!(expand_tilde("~/bots-data"), home.join("bots-data"));
        assert_eq!(
            normalize_host("http://127.0.0.1:31009/").unwrap(),
            DEFAULT_HOST
        );
    }

    #[test]
    fn tunnel_and_remote_callers_cannot_change_storage() {
        let mut headers = HeaderMap::new();
        let local: SocketAddr = "127.0.0.1:9".parse().unwrap();
        assert!(ensure_local(local, &headers, None).is_ok());
        headers.insert("x-forwarded-for", "203.0.113.4".parse().unwrap());
        assert_eq!(
            ensure_local(local, &headers, None).unwrap_err().status,
            StatusCode::FORBIDDEN
        );
        let remote: SocketAddr = "203.0.113.8:9".parse().unwrap();
        assert_eq!(
            ensure_local(remote, &HeaderMap::new(), Some(&Surface::Tailnet))
                .unwrap_err()
                .status,
            StatusCode::FORBIDDEN
        );
    }
}
