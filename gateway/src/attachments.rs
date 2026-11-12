//! Files a person attaches to a Bot's chat.
//!
//! The bytes live in `bots/<slug>/.bot/attachments/<id>.bin` (mode 600) with a small JSON
//! sidecar. `message.dispatch` takes T3's chat attachment (`type`, `id`, `name`, `mimeType`,
//! `sizeBytes`). `type` is `image` when the media type starts with `image/`, otherwise `file`.
//! The bytes themselves are not copied into the command.

use crate::error::{ApiError, ApiResult};
use crate::vault::now_iso;
use crate::{parse, require_json, AppState};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};

const MAX_BYTES: usize = 8 << 20;
const MAX_FILES: usize = 8;

pub(crate) async fn list(
    State(s): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> ApiResult<Json<Value>> {
    ensure_bot(&s, &slug).await?;
    Ok(Json(
        json!({ "attachments": read_dir(&s.cfg.bots_root, &slug).await? }),
    ))
}

pub(crate) async fn create(
    State(s): State<Arc<AppState>>,
    Path(slug): Path<String>,
    h: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    require_json(&h)?;
    ensure_bot(&s, &slug).await?;
    let (name, media, bytes) = prepare(&parse(&body)?)?;
    let saved = write_file(&s.cfg.bots_root, &slug, &name, &media, &bytes).await?;
    s.usage
        .note_for(&slug, crate::usage::Event::Attachment)
        .await;
    Ok(Json(saved))
}

pub(crate) async fn download(
    State(s): State<Arc<AppState>>,
    Path((slug, id)): Path<(String, String)>,
) -> ApiResult<Response> {
    ensure_bot(&s, &slug).await?;
    let (meta, bytes) = read_file(&s.cfg.bots_root, &slug, &id).await?;
    let media = meta["mediaType"]
        .as_str()
        .unwrap_or("application/octet-stream");
    let name = meta["name"].as_str().unwrap_or("file");
    // HeaderValue's error echoes the string it rejected. A stored type or name must
    // not come back to the client; a bad header is a server fault (500).
    #[allow(
        clippy::map_err_ignore,
        reason = "the header parser echoes the media type or file name, and the client only gets a 500"
    )]
    let ctype = HeaderValue::from_str(media).map_err(|_| ApiError::new(500, "bad media type"))?;
    #[allow(
        clippy::map_err_ignore,
        reason = "the header parser echoes the media type or file name, and the client only gets a 500"
    )]
    let disposition =
        HeaderValue::from_str(&format!("attachment; filename=\"{}\"", file_token(name)))
            .map_err(|_| ApiError::new(500, "bad file name"))?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, ctype),
            (header::CONTENT_DISPOSITION, disposition),
            (
                header::HeaderName::from_static("x-content-type-options"),
                HeaderValue::from_static("nosniff"),
            ),
        ],
        Bytes::from(bytes),
    )
        .into_response())
}

pub(crate) async fn remove(
    State(s): State<Arc<AppState>>,
    Path((slug, id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    ensure_bot(&s, &slug).await?;
    delete_file(&s.cfg.bots_root, &slug, &id).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn ensure_bot(s: &AppState, slug: &str) -> ApiResult<()> {
    if s.bots.get(slug).await?.is_none() {
        return Err(ApiError::not_found("Bot not found"));
    }
    Ok(())
}

/// Name, media type and decoded bytes from a JSON body. Nothing is written.
pub(crate) fn prepare(body: &Value) -> ApiResult<(String, String, Vec<u8>)> {
    let name = clean_name(body["name"].as_str().unwrap_or(""))?;
    let media = clean_media(body["mediaType"].as_str().unwrap_or(""))?;
    let data = body["data"].as_str().unwrap_or("");
    if data.len() > MAX_BYTES * 2 {
        return Err(ApiError::bad_request("the file is too large"));
    }
    #[allow(
        clippy::map_err_ignore,
        reason = "a base64 error quotes the payload, which is the uploaded file"
    )]
    let bytes = B64
        .decode(data.trim())
        .map_err(|_| ApiError::bad_request("data must be base64"))?;
    if bytes.is_empty() {
        return Err(ApiError::bad_request("the file is empty"));
    }
    if bytes.len() > MAX_BYTES {
        return Err(ApiError::bad_request("the file is too large"));
    }
    Ok((name, media, bytes))
}

pub(crate) fn clean_name(name: &str) -> ApiResult<String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(ApiError::bad_request("give the file a name"));
    }
    if name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
    {
        return Err(ApiError::bad_request("that file name is not allowed"));
    }
    if name.chars().any(|c| c.is_control()) {
        return Err(ApiError::bad_request("that file name is not allowed"));
    }
    Ok(name.to_string())
}

pub(crate) fn clean_media(media: &str) -> ApiResult<String> {
    let media = media.trim().to_ascii_lowercase();
    let ok = matches!(
        media.as_str(),
        "image/png"
            | "image/jpeg"
            | "image/gif"
            | "image/webp"
            | "text/plain"
            | "text/markdown"
            | "application/pdf"
            | "application/json"
            | "audio/webm"
            | "audio/mpeg"
            | "audio/wav"
            | "audio/mp4"
            | "video/mp4"
            | "video/webm"
            | "application/octet-stream"
    );
    if !ok {
        return Err(ApiError::bad_request("that file type is not allowed"));
    }
    Ok(media)
}

pub(crate) fn command_part(meta: &Value) -> Value {
    let media = meta["mediaType"]
        .as_str()
        .unwrap_or("application/octet-stream");
    let kind = if media.to_ascii_lowercase().starts_with("image/") {
        "image"
    } else {
        "file"
    };
    json!({
        "type": kind,
        "id": meta["id"],
        "name": meta["name"],
        "mimeType": media,
        "sizeBytes": meta["bytes"],
    })
}

/// Ids from a send body, capped, each one a real stored file.
pub(crate) async fn for_send(
    root: &std::path::Path,
    slug: &str,
    body: &Value,
) -> ApiResult<Vec<Value>> {
    let ids: Vec<&str> = body["attachmentIds"]
        .as_array()
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if ids.len() > MAX_FILES {
        return Err(ApiError::bad_request(format!(
            "attach at most {MAX_FILES} files"
        )));
    }
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let (meta, _) = read_file(root, slug, id).await?;
        out.push(command_part(&meta));
    }
    Ok(out)
}

pub(crate) async fn write_file(
    root: &std::path::Path,
    slug: &str,
    name: &str,
    media: &str,
    bytes: &[u8],
) -> ApiResult<Value> {
    let id = uuid::Uuid::new_v4().to_string();
    let dir = dir_for(root, slug)?;
    tokio::fs::create_dir_all(&dir).await?;
    let meta = json!({
        "id": id,
        "name": name,
        "mediaType": media,
        "bytes": bytes.len(),
        "createdAt": now_iso(),
    });
    crate::storage::write_atomic(&bin_path(&dir, &id)?, bytes, Some(0o600)).await?;
    crate::storage::write_json(&json_path(&dir, &id)?, &meta).await?;
    Ok(meta)
}

pub(crate) async fn read_file(
    root: &std::path::Path,
    slug: &str,
    id: &str,
) -> ApiResult<(Value, Vec<u8>)> {
    let dir = dir_for(root, slug)?;
    let meta = crate::storage::read_json(&json_path(&dir, id)?)
        .await
        .ok_or_else(|| ApiError::not_found("no such file"))?;
    #[allow(
        clippy::map_err_ignore,
        reason = "the IO error includes the on-disk path; the client only hears that the file is missing"
    )]
    let bytes = tokio::fs::read(bin_path(&dir, id)?)
        .await
        .map_err(|_| ApiError::not_found("no such file"))?;
    Ok((meta, bytes))
}

pub(crate) async fn read_dir(root: &std::path::Path, slug: &str) -> ApiResult<Vec<Value>> {
    let dir = dir_for(root, slug)?;
    let mut entries = match tokio::fs::read_dir(&dir).await {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        if let Some(meta) = crate::storage::read_json(&json_path(&dir, id)?).await {
            out.push(meta);
        }
    }
    out.sort_by(|a, b| a["createdAt"].as_str().cmp(&b["createdAt"].as_str()));
    Ok(out)
}

pub(crate) async fn delete_file(root: &std::path::Path, slug: &str, id: &str) -> ApiResult<()> {
    let dir = dir_for(root, slug)?;
    let json = json_path(&dir, id)?;
    if tokio::fs::metadata(&json).await.is_err() {
        return Err(ApiError::not_found("no such file"));
    }
    let _ = tokio::fs::remove_file(bin_path(&dir, id)?).await;
    tokio::fs::remove_file(json).await?;
    Ok(())
}

fn dir_for(root: &std::path::Path, slug: &str) -> ApiResult<PathBuf> {
    if !crate::bots::valid_slug(slug) {
        return Err(ApiError::bad_request("bad slug"));
    }
    Ok(root.join(slug).join(".bot").join("attachments"))
}

fn id_ok(id: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok()
}

fn json_path(dir: &std::path::Path, id: &str) -> ApiResult<PathBuf> {
    if !id_ok(id) {
        return Err(ApiError::bad_request("bad file id"));
    }
    Ok(dir.join(format!("{id}.json")))
}

fn bin_path(dir: &std::path::Path, id: &str) -> ApiResult<PathBuf> {
    if !id_ok(id) {
        return Err(ApiError::bad_request("bad file id"));
    }
    Ok(dir.join(format!("{id}.bin")))
}

/// A filename token for Content-Disposition: no quotes or newlines.
fn file_token(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_types() {
        assert_eq!(clean_name(" notes.txt ").unwrap(), "notes.txt");
        assert!(clean_name("../x").is_err());
        assert!(clean_name("a/b").is_err());
        assert!(clean_name("").is_err());
        assert_eq!(clean_media("Image/PNG").unwrap(), "image/png");
        assert!(clean_media("text/html").is_err());
        assert!(clean_media("application/javascript").is_err());
    }

    #[test]
    fn prepare_decodes_and_refuses_a_bad_body() {
        let data = B64.encode(b"hello");
        let (name, media, bytes) = prepare(&json!({
            "name": "hello.txt", "mediaType": "text/plain", "data": data,
        }))
        .unwrap();
        assert_eq!(
            (name.as_str(), media.as_str(), bytes.as_slice()),
            ("hello.txt", "text/plain", b"hello".as_slice())
        );
        assert!(
            prepare(&json!({ "name": "x.txt", "mediaType": "text/plain", "data": "!!!" })).is_err()
        );
        assert!(
            prepare(&json!({ "name": "x.txt", "mediaType": "text/html", "data": data })).is_err()
        );
    }

    #[tokio::test]
    async fn a_file_round_trips_and_the_command_names_it() {
        let dir = tempfile::tempdir().unwrap();
        let meta = write_file(dir.path(), "ada", "photo.png", "image/png", b"png-bytes")
            .await
            .unwrap();
        let id = meta["id"].as_str().unwrap();
        let (again, bytes) = read_file(dir.path(), "ada", id).await.unwrap();
        assert_eq!(bytes, b"png-bytes");
        assert_eq!(again["name"], "photo.png");
        let bin = dir
            .path()
            .join("ada")
            .join(".bot")
            .join("attachments")
            .join(format!("{id}.bin"));
        let mode = std::fs::metadata(&bin).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(mode.mode() & 0o777, 0o600);
        }
        let part = command_part(&meta);
        assert_eq!(part["type"], "image");
        assert_eq!(part["id"], id);
        assert_eq!(part["mimeType"], "image/png");
        assert_eq!(part["sizeBytes"], b"png-bytes".len());
        assert!(part.get("data").is_none());
        let text = command_part(&json!({
            "id": id, "name": "notes.txt", "mediaType": "text/plain", "bytes": 4,
        }));
        assert_eq!(text["type"], "file");
        assert_eq!(text["mimeType"], "text/plain");
        assert_eq!(text["sizeBytes"], 4);
        let listed = read_dir(dir.path(), "ada").await.unwrap();
        assert_eq!(listed.len(), 1);
        let send = for_send(dir.path(), "ada", &json!({ "attachmentIds": [id] }))
            .await
            .unwrap();
        assert_eq!(send[0]["name"], "photo.png");
        delete_file(dir.path(), "ada", id).await.unwrap();
        assert!(read_file(dir.path(), "ada", id).await.is_err());
        let _ = mode;
    }
}
