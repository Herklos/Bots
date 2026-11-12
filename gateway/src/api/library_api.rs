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
    } else if is("gif") {
        "image/gif"
    } else if is("webp") {
        "image/webp"
    } else {
        "application/octet-stream"
    }
}

pub(crate) async fn library_file(
    State(s): S,
    Path(slug): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    require_bot(&s, &slug).await?;
    let path = q.get("path").map(String::as_str).unwrap_or("");
    let bytes = s
        .library
        .file_bytes(&slug, path)
        .await
        .map_err(crate::bots::library::FileError::api)?;
    let download = q.get("download").is_some_and(|value| value == "1");
    Ok(library_response(&headers, path, bytes, download))
}

fn library_response(headers: &HeaderMap, path: &str, bytes: Vec<u8>, download: bool) -> Response {
    let mime = library_mime(path);
    let tag = crate::chat::images::etag_of(&bytes);
    let seen = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok());
    let mut res = if crate::chat::none_match(seen, &tag) {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        (StatusCode::OK, bytes).into_response()
    };
    let etag = HeaderValue::from_str(&tag).unwrap_or(HeaderValue::from_static("\"0\""));
    res.headers_mut().insert(header::ETAG, etag);
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-cache"),
    );
    res.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(content_type) = HeaderValue::from_str(mime) {
        res.headers_mut().insert(header::CONTENT_TYPE, content_type);
    }
    // A Bot-written page must not run script in the person's session. `sandbox`
    // (no allow-scripts, no allow-same-origin) does that. Images stay unsandboxed:
    // a sandbox policy on an image response can stop the page from showing it.
    if mime.starts_with("text/html") {
        res.headers_mut().insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("sandbox"),
        );
    }
    if let Some(disposition) = content_disposition(path, download) {
        res.headers_mut()
            .insert(header::CONTENT_DISPOSITION, disposition);
    }
    res
}

fn content_disposition(path: &str, download: bool) -> Option<HeaderValue> {
    let image = crate::chat::images::kind_of(path).is_some();
    if !image && !download {
        return None;
    }
    let name = path.rsplit('/').next().unwrap_or("file");
    let token: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let kind = if download { "attachment" } else { "inline" };
    HeaderValue::from_str(&format!("{kind}; filename=\"{token}\"")).ok()
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
