//! Text of one tool call. Vault calls return nothing. Everything else is scrubbed and capped.
use crate::platform::redact::{redact, Secrets};
use regex::Regex;
use serde_json::Value;

const DETAIL_BYTES: usize = 2048;

pub(super) fn input_of(item: &Value) -> Value {
    let input = read_field(&item["input"]);
    if input.as_object().is_some_and(|fields| !fields.is_empty()) || input.is_string() {
        return input;
    }
    item.get("command")
        .cloned()
        .or_else(|| item.get("arguments").cloned())
        .unwrap_or(Value::Null)
}

pub(super) fn output_of(item: &Value) -> Value {
    for key in ["output", "result", "stdout", "content"] {
        if let Some(value) = item.get(key).filter(|value| !value.is_null()) {
            return read_field(value);
        }
    }
    Value::Null
}

/// A JSON object sent as text is an object. Plain text stays text.
fn read_field(value: &Value) -> Value {
    match value {
        Value::String(text) => serde_json::from_str::<Value>(text)
            .ok()
            .filter(|parsed| parsed.is_object() || parsed.is_array())
            .unwrap_or_else(|| Value::String(text.clone())),
        other => other.clone(),
    }
}

pub(super) fn clip(value: &Value) -> (String, bool) {
    cap(&scrub(&text_of(value)))
}

pub(super) fn scrub(text: &str) -> String {
    let blank = Secrets {
        token: None,
        password: None,
        extras: Vec::new(),
        home: None,
    };
    let mut out = redact(text, &blank);
    let patterns = [
        (r"(?i)(password\s*[=:]\s*)\S+", "$1[redacted]"),
        (r"\bsk-[A-Za-z0-9_\-]{8,}", "[redacted]"),
        (r"\bghp_[A-Za-z0-9]{8,}", "[redacted]"),
        (r"[A-Za-z0-9+/]{40,}={0,2}", "[redacted]"),
    ];
    for (pattern, replacement) in patterns {
        let re = Regex::new(pattern).expect("activity redaction pattern");
        out = re.replace_all(&out, replacement).into_owned();
    }
    out
}

fn text_of(value: &Value) -> String {
    if value.is_null() {
        return String::new();
    }
    if let Some(text) = value.as_str() {
        return text.to_string();
    }
    if is_image(value) {
        return "image".to_string();
    }
    if is_file(value) {
        return "file".to_string();
    }
    if let Some(object) = value.as_object() {
        for key in ["text", "stdout", "output", "content", "result"] {
            if let Some(text) = object.get(key).and_then(Value::as_str) {
                return text.to_string();
            }
        }
    }
    value.to_string()
}

fn is_image(value: &Value) -> bool {
    value["type"].as_str() == Some("image")
        || value["mime"]
            .as_str()
            .or(value["mimeType"].as_str())
            .is_some_and(|mime| mime.starts_with("image/"))
}

fn is_file(value: &Value) -> bool {
    value["type"].as_str() == Some("file")
        || value["mime"]
            .as_str()
            .or(value["mimeType"].as_str())
            .is_some_and(|mime| mime == "application/octet-stream" || mime.starts_with("audio/"))
}

fn cap(text: &str) -> (String, bool) {
    if text.len() <= DETAIL_BYTES {
        return (text.to_string(), false);
    }
    let mut end = DETAIL_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), true)
}
