//! Object bodies for `m_file`. Text at most 256 KiB stays in `m_content`.
//! Anything larger, or any byte that is not UTF-8, goes up as a file and `m_blob` holds its id.

use super::outbox::Op;
use super::sha256_hex;
use serde_json::{json, Value};

pub(super) const TEXT_LIMIT: usize = 256 * 1024;

pub(super) enum Kind {
    Text(String),
    Blob,
}

pub(super) fn kind(bytes: &[u8]) -> Kind {
    if bytes.len() > TEXT_LIMIT {
        return Kind::Blob;
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => Kind::Text(text.to_string()),
        Err(_) => Kind::Blob,
    }
}

pub(super) fn is_text(bytes: &[u8]) -> bool {
    matches!(kind(bytes), Kind::Text(_))
}

/// A patch retries with a new `If-Match`, so the wire key includes the etag.
/// The outbox keeps the logical key (`device | path | op | sha`) and dedups on that.
pub(super) fn wire_key(logical: &str, etag: Option<&str>) -> String {
    match etag {
        Some(etag) if !etag.is_empty() => format!("{logical}:{etag}"),
        Some(_) | None => logical.to_string(),
    }
}

pub(super) fn blob_key(device: &str, rel: &str, sha: &str) -> String {
    super::outbox::idem_key(device, &format!("blob:{rel}"), Op::Create, sha)
}

pub(super) fn owner(rel: &str) -> String {
    let first = rel.split('/').next().unwrap_or("");
    if first.starts_with('.') {
        String::new()
    } else {
        first.to_string()
    }
}

pub(super) fn dir_of(rel: &str) -> String {
    match rel.rsplit_once('/') {
        Some((dir, _)) => dir.to_string(),
        None => String::new(),
    }
}

pub(super) fn mime_of(rel: &str, text: bool) -> &'static str {
    if !text {
        return "application/octet-stream";
    }
    if ext_is(rel, "md") {
        "text/markdown"
    } else if ext_is(rel, "html") {
        "text/html"
    } else if ext_is(rel, "json") {
        "application/json"
    } else {
        "text/plain"
    }
}

fn ext_is(rel: &str, ext: &str) -> bool {
    rel.rsplit_once('.')
        .is_some_and(|(_, found)| found.eq_ignore_ascii_case(ext))
}

pub(super) fn file_properties(
    rel: &str,
    bytes: &[u8],
    deleted: bool,
    device: &str,
    rev: i64,
    mtime_ns: i64,
    blob: Option<&str>,
) -> Value {
    let sha = sha256_hex(bytes);
    let size = i64::try_from(bytes.len()).unwrap_or(i64::MAX);
    let text = is_text(bytes) && blob.is_none() && !deleted;
    let content = if text {
        String::from_utf8(bytes.to_vec()).unwrap_or_default()
    } else {
        String::new()
    };
    let blob_value = match blob {
        Some(id) if !deleted && !id.is_empty() => json!([id]),
        Some(_) | None => json!([]),
    };
    json!({
        "m_path": rel,
        "m_dir": dir_of(rel),
        "m_content": content,
        "m_sha": sha,
        "m_size": if deleted { 0 } else { size },
        "m_mime": mime_of(rel, text),
        "m_mtime": mtime_ns / 1_000_000_000,
        "m_deleted": deleted,
        "m_owner_bot": owner(rel),
        "m_device": device,
        "m_rev": rev,
        "m_blob": blob_value,
    })
}

pub(super) fn create_body(properties: Value) -> Value {
    json!({
        "type": "m_file",
        "name": properties["m_path"].as_str().unwrap_or(""),
        "properties": properties,
    })
}

pub(super) fn patch_body(properties: Value) -> Value {
    json!({
        "ops": [{
            "op": "set_properties",
            "set": properties,
        }],
    })
}

pub(super) fn etag_of(value: &Value) -> Option<String> {
    value
        .get("etag")
        .and_then(Value::as_str)
        .filter(|etag| !etag.is_empty())
        .map(str::to_string)
}

pub(super) fn id_of(value: &Value) -> Option<String> {
    value
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}
