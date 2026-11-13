use super::*;

pub(crate) const KINDS: &[&str] = &[
    "project", "task", "machine", "service", "repo", "note", "link",
    "epic", "user_story",
];

pub(crate) const LINK_TYPES: &[&str] = &["thread", "bot", "item", "url", "t3project"];

pub(crate) const PRIORITIES: &[&str] = &["high", "normal", "low"];

pub(crate) const FIELD_NAMES: &[&str] = &[
    "url", "path", "host", "port", "branch", "due", "priority", "owner", "tags",
];

pub(crate) const MAX_BATCH: usize = 100;

pub(crate) const MAX_ITEMS: usize = 5000;

pub(crate) const TRACKED: &[&str] = &[
    "kind",
    "title",
    "status",
    "area",
    "body",
    "parentId",
    "machineId",
    "fields",
    "links",
    "pinned",
    "order",
    "archived",
];

pub(crate) const ID_ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";

/// The statuses a kind may have. Note and link have none.
pub(crate) fn statuses(kind: &str) -> &'static [&'static str] {
    match kind {
        "project" => &["active", "paused", "idea", "done"],
        "task" => &["todo", "doing", "waiting", "done"],
        "machine" => &["online", "offline", "unknown"],
        "service" => &["running", "stopped", "failing", "unknown"],
        "repo" => &["active", "archived"],
        "epic" | "user_story" => &["backlog", "ready", "doing", "done", "cancelled"],
        _ => &[],
    }
}

pub(crate) fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::bad_request(message)
}

pub(crate) fn unreadable() -> ApiError {
    ApiError::new(500, "The Projects board file can't be read.")
}

pub(crate) fn kind_error() -> ApiError {
    invalid(format!("kind must be one of {}.", KINDS.join(", ")))
}

pub(crate) fn quoted(v: &Value) -> String {
    format!(
        "\"{}\"",
        v.as_str().map_or_else(|| v.to_string(), String::from)
    )
}

pub(crate) fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

pub(crate) fn as_list(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(list) => list.clone(),
        other => vec![other.clone()],
    }
}

/// Text as JavaScript's cleaners see it: CRLF folded, trimmed, bounded; empty means absent.
pub(crate) fn text(v: &Value, field: &str, max: usize) -> ApiResult<Option<String>> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) => {
            let t = s.replace("\r\n", "\n");
            let t = t.trim();
            if t.chars().count() > max {
                return Err(invalid(format!("{field} is longer than {max} characters.")));
            }
            Ok((!t.is_empty()).then(|| t.to_string()))
        }
        _ => Err(invalid(format!("{field} must be text."))),
    }
}

/// One line: like `text`, with runs of whitespace collapsed.
pub(crate) fn oneline(v: &Value, field: &str, max: usize) -> ApiResult<Option<String>> {
    Ok(text(v, field, max)?.map(|t| t.split_whitespace().collect::<Vec<_>>().join(" ")))
}

pub(crate) fn clean_text(v: &Value, field: &str, max: usize) -> ApiResult<Option<Value>> {
    Ok(oneline(v, field, max)?.map(Value::String))
}

pub(crate) fn clean_url(v: &Value, field: &str) -> ApiResult<Option<String>> {
    let Some(s) = oneline(v, field, 2000)? else {
        return Ok(None);
    };
    #[allow(
        clippy::map_err_ignore,
        reason = "the URL parser echoes the address the user typed, which the reply already refuses"
    )]
    let url = url::Url::parse(&s).map_err(|_| {
        invalid(format!(
            "{field} must be a full address, like https://example.com."
        ))
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid(format!(
            "{field} must start with http:// or https://."
        )));
    }
    Ok(Some(url.to_string()))
}

pub(crate) fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 {
        return false;
    }
    let shape_ok = b.iter().enumerate().all(|(i, c)| match i {
        4 | 7 => *c == b'-',
        _ => c.is_ascii_digit(),
    });
    if !shape_ok {
        return false;
    }
    let (Ok(y), Ok(m), Ok(d)) = (
        s[0..4].parse::<u32>(),
        s[5..7].parse::<u32>(),
        s[8..10].parse::<u32>(),
    ) else {
        return false;
    };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&d)
}

pub(crate) fn clean_date(v: &Value) -> ApiResult<Option<Value>> {
    let Some(s) = oneline(v, "due", 10)? else {
        return Ok(None);
    };
    if !valid_date(&s) {
        return Err(invalid("due must be a date like 2026-11-03."));
    }
    Ok(Some(json!(s)))
}

pub(crate) fn clean_tags(v: &Value) -> ApiResult<Option<Value>> {
    if v.is_null() {
        return Ok(None);
    }
    let list = v
        .as_array()
        .ok_or_else(|| invalid("tags must be a list of words."))?;
    let mut tags: Vec<String> = Vec::new();
    for raw in list {
        if raw.is_null() {
            continue;
        }
        let Some(tag) = oneline(raw, "tag", 32)?.map(|t| t.to_lowercase()) else {
            continue;
        };
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    if tags.len() > 12 {
        return Err(invalid("Use at most 12 tags."));
    }
    Ok((!tags.is_empty()).then(|| json!(tags)))
}

pub(crate) fn clean_port(v: &Value) -> ApiResult<Option<Value>> {
    if v.is_null() || v.as_str() == Some("") {
        return Ok(None);
    }
    let n = match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    };
    match n {
        Some(n) if n.fract() == 0.0 && (1.0..=65535.0).contains(&n) => {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "port is a whole number from 1 to 65535"
            )]
            let port = n as u64;
            Ok(Some(json!(port)))
        }
        _ => Err(invalid("port must be a number from 1 to 65535.")),
    }
}

pub(crate) fn clean_priority(v: &Value) -> ApiResult<Option<Value>> {
    if v.is_null() || v.as_str() == Some("") {
        return Ok(None);
    }
    match v.as_str().filter(|p| PRIORITIES.contains(p)) {
        Some(p) => Ok(Some(json!(p))),
        None => Err(invalid(format!(
            "priority must be one of {}.",
            PRIORITIES.join(", ")
        ))),
    }
}

/// Merge `input` into the item's existing fields. Unknown names are refused; null removes one.
pub(crate) fn clean_fields(
    input: &Value,
    previous: Map<String, Value>,
) -> ApiResult<Map<String, Value>> {
    if input.is_null() {
        return Ok(Map::new());
    }
    let obj = input
        .as_object()
        .ok_or_else(|| invalid("fields must be an object."))?;
    let mut next = previous;
    for (key, value) in obj {
        let cleaned = match key.as_str() {
            "url" => clean_url(value, "url")?.map(Value::String),
            "path" => clean_text(value, "path", 500)?,
            "host" => clean_text(value, "host", 200)?,
            "port" => clean_port(value)?,
            "branch" => clean_text(value, "branch", 200)?,
            "due" => clean_date(value)?,
            "priority" => clean_priority(value)?,
            "owner" => clean_text(value, "owner", 60)?,
            "tags" => clean_tags(value)?,
            _ => {
                return Err(invalid(format!(
                    "Unknown field \"{key}\". Use {}, or put it in body.",
                    FIELD_NAMES.join(", ")
                )))
            }
        };
        match cleaned {
            Some(v) => {
                next.insert(key.clone(), v);
            }
            None => {
                next.remove(key);
            }
        }
    }
    Ok(next)
}

pub(crate) fn env_id(v: &Value) -> ApiResult<Option<String>> {
    let Some(env) = oneline(v, "environmentId", 128)? else {
        return Ok(None);
    };
    let ok = env.len() <= 128
        && env
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !ok {
        return Err(invalid("environmentId looks wrong."));
    }
    Ok(Some(env))
}

pub(crate) fn valid_slug(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 63
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

pub(crate) fn same_value(a: Option<&Value>, b: Option<&Value>) -> bool {
    a.unwrap_or(&Value::Null) == b.unwrap_or(&Value::Null)
}

pub(crate) fn new_id(taken: &HashSet<String>) -> String {
    loop {
        let id: String = (0..8)
            .map(|_| char::from(ID_ALPHABET[usize::from(rand::random::<u8>()) % ID_ALPHABET.len()]))
            .collect();
        if !taken.contains(&id) {
            return id;
        }
    }
}
