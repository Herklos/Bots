use super::*;

pub(crate) async fn vault_list(State(s): S) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "items": s.vault.list().await? })))
}

pub(crate) async fn vault_get(
    State(s): S,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    Ok(Json(
        s.vault
            .get(&id, q.get("reveal").is_some_and(|v| v == "1"))
            .await?,
    ))
}

pub(crate) async fn vault_create(State(s): S, h: HeaderMap, body: Bytes) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let item = s.vault.create(&parse(&body)?).await?;
    s.usage.note(usage::Event::VaultWrite).await;
    Ok(Json(item))
}

pub(crate) async fn vault_update(
    State(s): S,
    Path(id): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let item = s.vault.update(&id, &parse(&body)?).await?;
    s.usage.note(usage::Event::VaultWrite).await;
    Ok(Json(item))
}

pub(crate) async fn vault_delete(State(s): S, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    s.vault.delete(&id).await?;
    s.usage.note(usage::Event::VaultWrite).await;
    Ok(Json(json!({ "ok": true })))
}
