//! Filter checks for a webhook routine. They mirror T3's own rules so the
//! gateway can refuse a bad filter before the upsert.

use serde_json::{json, Map, Value};

const FILTER_MAX: usize = 16;

pub(super) fn validate_filters(input: &Value) -> Result<Vec<Value>, &'static str> {
    let Some(raw) = input.get("filters").filter(|value| !value.is_null()) else {
        return Ok(Vec::new());
    };
    let list = raw.as_array().ok_or("Filters are a list.")?;
    if list.len() > FILTER_MAX {
        return Err("A webhook routine has at most 16 filters.");
    }
    list.iter().map(one_filter).collect()
}

enum FilterOp {
    Equals,
    NotEquals,
    StartsWith,
    In,
    Exists,
}

fn one_filter(value: &Value) -> Result<Value, &'static str> {
    let obj = value.as_object().ok_or("A filter is an object.")?;
    let path = obj
        .get("path")
        .and_then(Value::as_str)
        .ok_or("A filter path starts with body, headers, or query.")?;
    let path = normalize_path(path)?;
    let op = match obj.get("op").and_then(Value::as_str).unwrap_or("") {
        "equals" => FilterOp::Equals,
        "notEquals" => FilterOp::NotEquals,
        "startsWith" => FilterOp::StartsWith,
        "in" => FilterOp::In,
        "exists" => FilterOp::Exists,
        _ => return Err("A filter uses equals, notEquals, startsWith, in, or exists."),
    };
    match op {
        FilterOp::Exists => Ok(json!({ "path": path, "op": "exists" })),
        FilterOp::Equals => string_filter(&path, "equals", obj),
        FilterOp::NotEquals => string_filter(&path, "notEquals", obj),
        FilterOp::StartsWith => string_filter(&path, "startsWith", obj),
        FilterOp::In => in_filter(&path, obj),
    }
}

fn string_filter(path: &str, op: &str, obj: &Map<String, Value>) -> Result<Value, &'static str> {
    let text = obj
        .get("value")
        .and_then(Value::as_str)
        .ok_or("A filter value is text.")?;
    Ok(json!({ "path": path, "op": op, "value": text }))
}

fn in_filter(path: &str, obj: &Map<String, Value>) -> Result<Value, &'static str> {
    let values = obj
        .get("value")
        .and_then(Value::as_array)
        .ok_or("An in filter takes a list of values.")?;
    if values.is_empty() {
        return Err("An in filter needs a value.");
    }
    let mut out = Vec::with_capacity(values.len());
    for item in values {
        out.push(in_item(item)?);
    }
    Ok(json!({ "path": path, "op": "in", "value": out }))
}

fn in_item(value: &Value) -> Result<String, &'static str> {
    let text = value
        .as_str()
        .ok_or("An in filter takes a list of values.")?;
    if text.is_empty() {
        return Err("An in filter needs a value.");
    }
    if text.contains(',') {
        return Err("An in filter value cannot contain a comma.");
    }
    if text != text.trim() {
        return Err("An in filter value cannot start or end with a space.");
    }
    Ok(text.to_string())
}

/// `^(body|headers|query)(\.|$)`, with the header name lowercased.
fn normalize_path(path: &str) -> Result<String, &'static str> {
    let err = "A filter path starts with body, headers, or query.";
    let (head, rest) = if let Some(rest) = path.strip_prefix("headers") {
        ("headers", rest)
    } else if let Some(rest) = path.strip_prefix("body") {
        ("body", rest)
    } else if let Some(rest) = path.strip_prefix("query") {
        ("query", rest)
    } else {
        return Err(err);
    };
    if rest.is_empty() {
        return Ok(head.to_string());
    }
    if !rest.starts_with('.') {
        return Err(err);
    }
    if head == "headers" {
        Ok(format!("headers{}", rest.to_ascii_lowercase()))
    } else {
        Ok(format!("{head}{rest}"))
    }
}
