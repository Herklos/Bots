//! Space search: `type`, `query`, structured `filters`, compact `filter`, `fields`, `sorts`.
//!
//! A numeric `last_modified_date` filter matches nothing. A day-precision
//! `last_modified_date >= "YYYY-MM-DD"` compact filter matches rows that have the field.

use super::objects::document;
use super::state::Object;
use serde_json::{json, Value};

pub(super) fn search(objects: &[&Object], body: &Value) -> Vec<Value> {
    let query = body["query"].as_str().unwrap_or("").to_ascii_lowercase();
    let type_key = body["type"].as_str().filter(|value| !value.is_empty());
    let mut rows: Vec<&Object> = objects
        .iter()
        .copied()
        .filter(|object| !object.archived)
        .filter(|object| type_key.is_none_or(|key| object.type_key == key))
        .filter(|object| query.is_empty() || object.name.to_ascii_lowercase().contains(&query))
        .filter(|object| filters_match(object, body))
        .collect();
    if let Some(sorts) = body["sorts"].as_array() {
        for sort in sorts.iter().rev() {
            let property = sort["property"].as_str().unwrap_or("");
            let desc = sort["direction"].as_str() == Some("desc");
            rows.sort_by(|left, right| {
                let order = sort_key(left, property).cmp(&sort_key(right, property));
                if desc {
                    order.reverse()
                } else {
                    order
                }
            });
        }
    }
    rows.into_iter()
        .map(|object| project(object, body))
        .collect()
}

pub(super) fn reject_search_body(body: &Value) -> Option<Value> {
    if body.get("limit").is_some() {
        return Some(json!({
            "message": "resend with ?limit=25",
            "hint": "resend with ?limit=25",
        }));
    }
    if body.get("filters").is_some() && body.get("filter").is_some() {
        return Some(json!({"message": "Send filters or filter, not both."}));
    }
    if body["sorts"]
        .as_array()
        .is_some_and(|sorts| sorts.len() > 10)
    {
        return Some(json!({"message": "At most 10 sorts."}));
    }
    None
}

fn filters_match(object: &Object, body: &Value) -> bool {
    if let Some(filter) = body["filter"].as_str() {
        return compact_match(object, filter);
    }
    let Some(filters) = body["filters"].as_array() else {
        return true;
    };
    filters.iter().all(|node| node_match(object, node))
}

fn node_match(object: &Object, node: &Value) -> bool {
    if let Some(operator) = node["operator"].as_str() {
        let children = node["filters"].as_array();
        return match operator {
            "and" => children.is_none_or(|nodes| nodes.iter().all(|node| node_match(object, node))),
            "or" => children.is_some_and(|nodes| nodes.iter().any(|node| node_match(object, node))),
            _ => false,
        };
    }
    let property = node["property"].as_str().unwrap_or("");
    let condition = node["condition"].as_str().unwrap_or("");
    condition_match(object, property, condition, &node["value"])
}

fn condition_match(object: &Object, property: &str, condition: &str, value: &Value) -> bool {
    if property == "last_modified_date" && value.is_number() {
        return false;
    }
    if condition != "greater" {
        return false;
    }
    match (property_number(object, property), value.as_f64()) {
        (Some(current), Some(threshold)) => current > threshold,
        _ => false,
    }
}

fn compact_match(object: &Object, filter: &str) -> bool {
    let Some((property, rest)) = filter.split_once(">=") else {
        return false;
    };
    if property.trim() != "last_modified_date" {
        return false;
    }
    let date = rest.trim().trim_matches('"');
    if date.len() != 10 || date.as_bytes().get(4) != Some(&b'-') {
        return false;
    }
    object
        .last_modified
        .as_deref()
        .and_then(|modified| modified.get(..10))
        .is_some_and(|day| day >= date)
}

fn property_number(object: &Object, property: &str) -> Option<f64> {
    object.properties.get(property)?.as_f64()
}

fn number_key(number: &serde_json::Number) -> String {
    if let Some(value) = number.as_i64() {
        return format!("{value:020}");
    }
    if let Some(value) = number.as_u64() {
        return format!("{value:020}");
    }
    number.to_string()
}

fn sort_key(object: &Object, property: &str) -> String {
    if property == "name" {
        return object.name.clone();
    }
    if property == "last_modified_date" {
        return object.last_modified.clone().unwrap_or_default();
    }
    match object.properties.get(property) {
        Some(Value::Number(number)) => number_key(number),
        Some(Value::String(text)) => text.clone(),
        _ => String::new(),
    }
}

fn project(object: &Object, body: &Value) -> Value {
    let full = document(object);
    // Live Anytype omits `properties` unless `fields` names them. Match that
    // so schema_ready cannot lean on a search hit that production never sends.
    let Some(fields) = body["fields"].as_array() else {
        return json!({
            "id": full["id"],
            "name": full["name"],
            "type": full["type"],
        });
    };
    let mut picked = serde_json::Map::new();
    picked.insert("id".into(), full["id"].clone());
    picked.insert("name".into(), full["name"].clone());
    picked.insert("type".into(), full["type"].clone());
    let mut properties = serde_json::Map::new();
    for field in fields {
        let Some(name) = field.as_str() else {
            continue;
        };
        match name {
            "name" | "type" | "created_date" => {
                if let Some(value) = full.get(name) {
                    picked.insert(name.to_string(), value.clone());
                }
            }
            "last_modified_date" => {
                if let Some(value) = full.get(name) {
                    picked.insert(name.to_string(), value.clone());
                }
            }
            other => {
                if let Some(value) = full["properties"].get(other) {
                    properties.insert(other.to_string(), value.clone());
                }
            }
        }
    }
    if !properties.is_empty() {
        picked.insert("properties".into(), Value::Object(properties));
    }
    Value::Object(picked)
}
