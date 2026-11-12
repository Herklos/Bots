//! Images a Bot reply can show from its own `library/` folder.
//!
//! Exact draws `![alt](url)` as an image and fetches `http` and `https` sources.
//! Every image mark is therefore taken out of the text the client sees. A
//! `library/` file that passes the checks is added to the message and served
//! by the library route.

mod load;
mod refs;
mod resolver;
mod sniff;

#[cfg(test)]
mod tests;

pub(crate) use load::read_regular;
pub(crate) use refs::{library_rel, marks, strip};
pub(crate) use resolver::Resolver;
pub(crate) use sniff::{etag_of, image_id, kind_of, picture, Kind, MAX_IMAGE_BYTES};
#[cfg(test)]
pub(crate) use sniff::{gif_bytes, jpeg_bytes, png_bytes, webp_bytes};

use crate::storage::workspace::Workspace;
use resolver::ImageFile;
use serde_json::{json, Value};
use std::collections::HashSet;

/// Why a library path is not shown or not served.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImageFault {
    Missing,
    Link,
    TooBig,
    NotImage,
    BadPath,
    Io,
}

/// How many library images one message may show.
const PER_MESSAGE: usize = 8;

/// Unix milliseconds. A missing clock reads as zero and the short failure cache is used.
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Rewrites Bot messages in a chat view. A cache hit does no file IO.
pub(crate) async fn attach(
    view: &mut Value,
    resolver: &Resolver,
    ws: &dyn Workspace,
    slug: &str,
    workspace_root: &str,
    now_ms: u64,
) {
    let Some(messages) = view.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    for message in messages {
        if message["role"].as_str() != Some("bot") {
            continue;
        }
        let text = message["text"].as_str().unwrap_or("").to_string();
        if !text.contains("![") {
            continue;
        }
        let found = marks(&text, workspace_root);
        if found.is_empty() {
            continue;
        }
        let at = message["at"].as_str().and_then(refs::parse_time_ms);
        let mut shown = HashSet::new();
        let mut files = message
            .get("files")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut seen = Vec::new();
        for mark in &found {
            let Some(path) = mark.path.as_deref() else {
                continue;
            };
            if seen.iter().any(|kept: &String| kept == path) {
                continue;
            }
            if seen.len() == PER_MESSAGE {
                continue;
            }
            seen.push(path.to_string());
            if let Ok(image) = resolver.resolve(ws, slug, path, at, now_ms).await {
                shown.insert(path.to_string());
                files.push(image_json(&image));
            }
        }
        message["text"] = json!(strip(&text, &found, &shown));
        message["files"] = json!(files);
    }
}

fn image_json(image: &ImageFile) -> Value {
    json!({
        "id": image.id,
        "name": image.name,
        "mime": image.mime,
        "bytes": image.bytes,
        "kind": "image",
        "path": image.path,
        "width": image.width,
        "height": image.height,
    })
}
