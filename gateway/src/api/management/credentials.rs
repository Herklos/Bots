use super::*;
use sha2::{Digest, Sha256};

static WRITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// A second gateway may share this home. Use an OS lock as well as the
// in-process mutex so authentication's last-used write cannot undo revocation.
async fn home_lock(s: &AppState) -> ApiResult<std::fs::File> {
    let path = s.cfg.app_dir().join("management-mcp-credentials.lock");
    tokio::task::spawn_blocking(move || -> std::io::Result<std::fs::File> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path)?;
        file.lock()?;
        Ok(file)
    })
    .await
    .map_err(|_error| ApiError::new(500, "Cannot lock MCP credentials"))?
    .map_err(|_error| ApiError::new(500, "Cannot lock MCP credentials"))
}
fn file(s: &AppState) -> std::path::PathBuf {
    s.cfg.app_dir().join("management-mcp-credentials.json")
}
async fn load(s: &AppState) -> ApiResult<Value> {
    match tokio::fs::read(file(s)).await {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
            .ok()
            .filter(Value::is_object)
            .ok_or_else(|| ApiError::new(500, "MCP credential storage is invalid")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(_error) => Err(ApiError::new(500, "Cannot read MCP credentials")),
    }
}
fn digest(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}
async fn save(s: &AppState, value: &Value) -> ApiResult<()> {
    storage::write_atomic(&file(s), &serde_json::to_vec(value).unwrap(), Some(0o600)).await?;
    Ok(())
}
#[cfg(test)]
pub(super) async fn authorized(s: &AppState, h: &HeaderMap) -> bool {
    authenticate(s, h).await.is_some()
}
pub(super) async fn authenticate(s: &AppState, h: &HeaderMap) -> Option<permissions::Actor> {
    let token = bearer(h)?;
    let hashed = digest(token);
    let _lock = WRITE_LOCK.lock().await;
    let _home_lock = home_lock(s).await.ok()?;
    let mut saved = load(s).await.ok()?;
    let (id, scopes) = saved.as_object()?.iter().find_map(|(id, v)| {
        if !v["hash"]
            .as_str()
            .is_some_and(|stored| crate::network::auth::safe_equal(stored, &hashed))
        {
            return None;
        }
        Some((id.clone(), permissions::parse_scopes(v.get("scopes")).ok()?))
    })?;
    saved[&id]["lastUsedAt"] = json!(crate::vault::now_iso());
    save(s, &saved).await.ok()?;
    Some(permissions::Actor { id, scopes })
}
pub(super) async fn current_actor(s: &AppState, id: &str) -> Option<permissions::Actor> {
    let saved = load(s).await.ok()?;
    let record = saved.as_object()?.get(id)?.as_object()?;
    record.get("hash")?.as_str().filter(|hash| {
        hash.len() == 64
            && hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })?;
    Some(permissions::Actor {
        id: id.to_owned(),
        scopes: permissions::parse_scopes(record.get("scopes")).ok()?,
    })
}
pub(crate) async fn credentials_list(State(s): S) -> ApiResult<Json<Value>> {
    let saved = load(&s).await?;
    Ok(Json(
        json!({"endpoint":format!("http://127.0.0.1:{}/mcp",s.cfg.port),"credentials":saved.as_object().into_iter().flat_map(|m|m.iter()).map(|(id,v)|json!({"id":id,"label":v["label"],"scopes":permissions::parse_scopes(v.get("scopes")).unwrap_or_default(),"createdAt":v["createdAt"],"lastUsedAt":v["lastUsedAt"]})).collect::<Vec<_>>()}),
    ))
}
pub(crate) async fn credentials_create(
    State(s): S,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let label = body["label"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.chars().count() <= 120)
        .ok_or_else(|| ApiError::bad_request("A label of 1–120 characters is required"))?;
    let default_scopes = json!(["read"]);
    let scopes = permissions::parse_scopes(Some(body.get("scopes").unwrap_or(&default_scopes)))
        .map_err(ApiError::bad_request)?;
    use rand::RngCore;
    let mut random = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut random);
    let token = format!("hbmc_{}", hex::encode(random));
    let id = hex::encode(&random[..8]);
    let _lock = WRITE_LOCK.lock().await;
    let _home_lock = home_lock(&s).await?;
    let mut saved = load(&s).await?;
    saved[&id] = json!({"label":label,"hash":digest(&token),"scopes":scopes,"createdAt":crate::vault::now_iso(),"lastUsedAt":null});
    save(&s, &saved).await?;
    Ok(Json(json!({"id":id,"token":token})))
}
pub(crate) async fn credentials_delete(
    State(s): S,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let _lock = WRITE_LOCK.lock().await;
    let _home_lock = home_lock(&s).await?;
    let mut saved = load(&s).await?;
    if let Some(map) = saved.as_object_mut() {
        map.remove(&id);
    }
    save(&s, &saved).await?;
    Ok(Json(json!({"ok":true})))
}
