use super::*;

pub(crate) async fn list_side(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    Ok(Json(crate::chat::side::list(&s, &slug).await?))
}

pub(crate) async fn open_side(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let text = body["text"].as_str().unwrap_or("");
    Ok(Json(crate::chat::side::open(&s, &slug, text).await?))
}

pub(crate) async fn merge_side(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    let body = parse(&body)?;
    let thread_id = body["threadId"].as_str().unwrap_or("");
    Ok(Json(crate::chat::side::merge(&s, &slug, thread_id).await?))
}
