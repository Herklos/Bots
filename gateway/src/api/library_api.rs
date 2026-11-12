use super::*;

pub(crate) async fn library_get(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    require_bot(&s, &slug).await?;
    let messages = library_messages(&s, &slug).await;
    Ok(Json(s.library.view(&slug, &messages, unix_now()).await?))
}

/// What the panel's Library shows: pictures, files and links, read only.
pub(crate) async fn shelf_get(State(s): S, Path(slug): Path<String>) -> ApiResult<Json<Value>> {
    require_bot(&s, &slug).await?;
    let messages = library_messages(&s, &slug).await;
    let view = s.library.view(&slug, &messages, unix_now()).await?;
    Ok(Json(crate::bots::library::shelf_from(&slug, &view)))
}

pub(crate) async fn library_messages(s: &AppState, slug: &str) -> Vec<Value> {
    let Ok(thread) = main_thread(s, slug).await else {
        return Vec::new();
    };
    let Ok(transcript) = s.harness.transcript() else {
        return Vec::new();
    };
    let Ok(snapshot) = transcript.read(&thread).await else {
        return Vec::new();
    };
    snapshot.transcript()["messages"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

pub(crate) async fn library_save(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    require_bot(&s, &slug).await?;
    Ok(Json(
        s.library
            .save_item(&slug, &parse(&body)?, unix_now())
            .await?,
    ))
}

pub(crate) async fn library_delete(
    State(s): S,
    Path((slug, id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    require_bot(&s, &slug).await?;
    Ok(Json(s.library.delete_item(&slug, &id).await?))
}

/// Content type for a library file. The extension match ignores case: the default macOS volume does too.
pub(crate) fn library_mime(path: &str) -> &'static str {
    let ext = std::path::Path::new(path).extension();
    let is = |want: &str| ext.is_some_and(|found| found.eq_ignore_ascii_case(want));
    if is("html") || is("htm") {
        "text/html; charset=utf-8"
    } else if is("png") {
        "image/png"
    } else if is("jpg") || is("jpeg") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    }
}

pub(crate) async fn library_file(
    State(s): S,
    Path(slug): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    require_bot(&s, &slug).await?;
    let path = q.get("path").map(String::as_str).unwrap_or("");
    let bytes = s.library.file_bytes(&slug, path).await?;
    let mime = library_mime(path);
    Ok(([(header::CONTENT_TYPE, mime)], bytes).into_response())
}

pub(crate) async fn connector_save(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    require_bot(&s, &slug).await?;
    Ok(Json(s.library.save_connector(&slug, &parse(&body)?).await?))
}

pub(crate) async fn connector_delete(
    State(s): S,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    require_bot(&s, &slug).await?;
    let name = parse(&body)?["name"].as_str().unwrap_or("").to_string();
    Ok(Json(s.library.delete_connector(&slug, &name).await?))
}
