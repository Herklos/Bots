//! Keep the API result intact and expose validated images as native MCP content.
use super::*;

pub(super) fn success(value: Value) -> Value {
    let mut content = vec![json!({"type":"text","text":value.to_string()})];
    let data = value["body"]["image"]
        .as_str()
        .or_else(|| value["body"]["bodyBase64"].as_str());
    if let Some(data) = data.filter(|data| !data.is_empty() && data.len() <= 8 << 20) {
        if let Ok(bytes) = STANDARD.decode(data) {
            use crate::chat::images::{picture, Kind};
            if let Some(image) = [Kind::Png, Kind::Jpeg, Kind::Gif, Kind::Webp]
                .into_iter()
                .find_map(|kind| picture(kind, &bytes))
            {
                content.push(json!({"type":"image","mimeType":image.mime,"data":data}));
            }
        }
    }
    json!({"content":content,"structuredContent":value})
}
