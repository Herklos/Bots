//! Pages a Bot published with `html_render`, and MCP apps a tool result already carries.
//! Both are read from the turn item's `output`. Nothing here asks the harness for bytes.

use serde_json::{json, Map, Value};

const MAX_RENDERS: usize = 20;
const MAX_PARSED_BYTES: usize = 16_384;
const MAX_DEPTH: u8 = 4;
const MAX_BLOCKS: usize = 32;
const MAX_TITLE_CHARS: usize = 200;
const MAX_ID_CHARS: usize = 256;
const MAX_URI_CHARS: usize = 4096;
const MAX_MEASURED: usize = 24;
const MIN_HEIGHT: i64 = 80;
const MAX_HEIGHT: i64 = 2000;

struct Payload {
    failed: bool,
    saw_data: bool,
    html: Option<Value>,
    app: Option<Value>,
}

impl Payload {
    fn empty() -> Self {
        Self {
            failed: false,
            saw_data: false,
            html: None,
            app: None,
        }
    }
}

/// HTML pages and MCP apps in `items`, oldest first. At most [`MAX_RENDERS`].
/// A missing or unexpected result is skipped. `messages` and `events` are left to the caller.
pub(crate) fn list(items: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    for (index, raw) in items.iter().enumerate() {
        let item = if raw["item"].is_object() {
            &raw["item"]
        } else {
            raw
        };
        if let Some(render) = one(item, index) {
            out.push(render);
            if out.len() == MAX_RENDERS {
                break;
            }
        }
    }
    out
}

fn one(item: &Value, index: usize) -> Option<Value> {
    if item["type"] != "dynamic_tool" || item["status"] != "completed" {
        return None;
    }
    let payload = payload(&item["output"], 0);
    if payload.failed {
        return None;
    }
    let name = item["toolName"].as_str().unwrap_or("");
    let fields = if is_html_render(name) {
        html_fields(payload.html.as_ref()?)?
    } else {
        app_fields(payload.app.as_ref()?)?
    };
    let mut row = Map::new();
    row.insert(
        "id".to_string(),
        json!(format!("render-{}", item["id"].as_str().unwrap_or(""))),
    );
    let ordinal = item["ordinal"]
        .as_i64()
        .unwrap_or_else(|| i64::try_from(index).unwrap_or(i64::MAX));
    row.insert("ordinal".to_string(), json!(ordinal));
    row.insert(
        "at".to_string(),
        json!(item["startedAt"]
            .as_str()
            .or(item["updatedAt"].as_str())
            .unwrap_or("")),
    );
    row.insert(
        "kind".to_string(),
        json!(if is_html_render(name) {
            "html"
        } else {
            "mcp-app"
        }),
    );
    row.extend(fields);
    Some(Value::Object(row))
}

/// `html_render` as `tool_of` sees it (`mcp__t3-code__html_render`), plus the dotted and
/// thread-scoped names the same result is stored under.
fn is_html_render(name: &str) -> bool {
    if name.rsplit("__").next() == Some("html_render") {
        return true;
    }
    let dotted = name.rsplit('.').next() == Some("html_render")
        && (name.starts_with("t3-code.") || name.starts_with("t3_code."));
    let scoped = name.starts_with("t3-code-thread_") && name.ends_with("_html_render");
    dotted || scoped
}

fn html_fields(value: &Value) -> Option<Map<String, Value>> {
    let attachment_id = bounded(value["attachmentId"].as_str()?, MAX_ID_CHARS)?;
    let title = value["title"].as_str()?;
    let height = clamp_height(value["height"].as_f64()?)?;
    let mut fields = Map::new();
    fields.insert("attachmentId".to_string(), json!(attachment_id));
    fields.insert("title".to_string(), json!(short_title(title)));
    fields.insert("height".to_string(), json!(height));
    if let Some(heights) = measured(value.get("heights")) {
        fields.insert("heights".to_string(), heights);
    }
    Some(fields)
}

fn short_title(title: &str) -> String {
    let trimmed = title.trim();
    let short: String = trimmed.chars().take(MAX_TITLE_CHARS).collect();
    if short.is_empty() {
        "HTML".to_string()
    } else {
        short
    }
}

fn measured(value: Option<&Value>) -> Option<Value> {
    let entries = value?.as_array()?;
    if entries.is_empty() || entries.len() > MAX_MEASURED {
        return None;
    }
    let mut pairs = Vec::with_capacity(entries.len());
    for entry in entries {
        let pair = entry.as_array()?;
        if pair.len() != 2 {
            return None;
        }
        let width = whole(pair.first()?)?;
        if !(1..=10_000).contains(&width) {
            return None;
        }
        let height = clamp_height(pair.get(1)?.as_f64()?)?;
        pairs.push((width, height));
    }
    pairs.sort_by_key(|(width, _)| *width);
    Some(json!(pairs
        .into_iter()
        .map(|(width, height)| json!([width, height]))
        .collect::<Vec<_>>()))
}

fn app_fields(value: &Value) -> Option<Map<String, Value>> {
    let attachment_id = bounded(value["attachmentId"].as_str()?, MAX_ID_CHARS)?;
    let server = bounded(value["server"].as_str()?, MAX_ID_CHARS)?;
    let tool = bounded(value["tool"].as_str()?, MAX_ID_CHARS)?;
    let uri = value["resourceUri"].as_str()?;
    if uri.chars().count() > MAX_URI_CHARS || !uri.starts_with("ui://") {
        return None;
    }
    let mut fields = Map::new();
    fields.insert("attachmentId".to_string(), json!(attachment_id));
    fields.insert("server".to_string(), json!(server));
    fields.insert("tool".to_string(), json!(tool));
    fields.insert("resourceUri".to_string(), json!(uri));
    if let Some(permissions) = permissions(value.get("permissions")) {
        fields.insert("permissions".to_string(), permissions);
    }
    if let Some(border) = value.get("prefersBorder").and_then(Value::as_bool) {
        fields.insert("prefersBorder".to_string(), json!(border));
    }
    Some(fields)
}

fn permissions(value: Option<&Value>) -> Option<Value> {
    let source = value?.as_object()?;
    let mut kept = Map::new();
    for key in ["camera", "microphone", "geolocation", "clipboardWrite"] {
        if source.get(key).is_some_and(Value::is_object) {
            kept.insert(key.to_string(), json!({}));
        }
    }
    if kept.is_empty() {
        None
    } else {
        Some(Value::Object(kept))
    }
}

fn bounded(value: &str, max: usize) -> Option<&str> {
    if value.trim().is_empty() || value.chars().count() > max {
        None
    } else {
        Some(value)
    }
}

fn whole(value: &Value) -> Option<i64> {
    if let Some(number) = value.as_i64() {
        return Some(number);
    }
    let number = value.as_f64()?;
    if number.is_finite() && number.fract() == 0.0 {
        float_to_i64(number)
    } else {
        None
    }
}

fn clamp_height(height: f64) -> Option<i64> {
    if !height.is_finite() {
        return None;
    }
    float_to_i64(
        height
            .round()
            .clamp(i64_as_f64(MIN_HEIGHT), i64_as_f64(MAX_HEIGHT)),
    )
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "the value is a finite whole number; the bounds reject anything outside i64"
)]
fn float_to_i64(value: f64) -> Option<i64> {
    if !value.is_finite() || value < i64::MIN as f64 || value > i64::MAX as f64 {
        return None;
    }
    Some(value as i64)
}

#[allow(clippy::cast_precision_loss, reason = "80 and 2000 are exact in f64")]
fn i64_as_f64(value: i64) -> f64 {
    value as f64
}

/// The object that holds `htmlRender` or `t3McpApp`, through the envelopes a tool result uses:
/// the object itself, a JSON string, `structuredContent`, or a list of text blocks.
fn payload(value: &Value, depth: u8) -> Payload {
    if depth > MAX_DEPTH {
        return Payload::empty();
    }
    match value {
        Value::String(text) => {
            if text.len() > MAX_PARSED_BYTES {
                return Payload::empty();
            }
            let Ok(parsed) = serde_json::from_str::<Value>(text) else {
                return Payload::empty();
            };
            payload(&parsed, depth.saturating_add(1))
        }
        Value::Array(blocks) => payload_blocks(blocks, depth),
        Value::Object(_) => payload_object(value, depth),
        _ => Payload::empty(),
    }
}

fn payload_blocks(blocks: &[Value], depth: u8) -> Payload {
    if blocks.len() > MAX_BLOCKS {
        return Payload::empty();
    }
    let mut found = Payload::empty();
    for block in blocks {
        let text = block.get("text").unwrap_or(block);
        let inner = if text.is_object() {
            text.get("text").unwrap_or(text)
        } else {
            text
        };
        let next = payload(inner, depth.saturating_add(1));
        found.failed |= next.failed;
        if !found.saw_data && next.saw_data {
            found.saw_data = true;
            found.html = next.html;
            found.app = next.app;
        }
    }
    found
}

fn payload_object(value: &Value, depth: u8) -> Payload {
    let failed = value["isError"] == true
        || value["is_error"] == true
        || value["_tag"] == "OrchestratorMcpFailure"
        || value.get("error").is_some_and(|error| !error.is_null());
    if let Some(nested) = value
        .get("structuredContent")
        .filter(|nested| !nested.is_null())
        .or_else(|| value.get("content").filter(|nested| !nested.is_null()))
    {
        let mut inner = payload(nested, depth.saturating_add(1));
        inner.failed |= failed;
        return inner;
    }
    Payload {
        failed,
        saw_data: true,
        html: value.get("htmlRender").cloned(),
        app: value.get("t3McpApp").cloned(),
    }
}
