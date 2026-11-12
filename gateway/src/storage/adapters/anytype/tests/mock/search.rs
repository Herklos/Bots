//! Space search over the in-memory objects: query, type, equality filters, field pick.

use super::state::{object_json, Object};
use serde_json::Value;

pub(super) fn search(objects: &[&Object], body: &Value) -> Vec<Value> {
    let query = body["query"].as_str().unwrap_or("").to_ascii_lowercase();
    let type_key = body["type"].as_str().filter(|value| !value.is_empty());
    let filters = body["filters"].as_array();
    let mut rows: Vec<&Object> = objects
        .iter()
        .copied()
        .filter(|object| !object.archived)
        .filter(|object| type_key.map(|key| object.type_key == key).unwrap_or(true))
        .filter(|object| query.is_empty() || object.name.to_ascii_lowercase().contains(&query))
        .filter(|object| filters_match(object, filters))
        .collect();
    if let Some(key) = body["sorts"].as_array().and_then(|sorts| sorts.first()) {
        let property = key["key"].as_str().unwrap_or("name");
        let desc = key["order"].as_str() == Some("desc");
        rows.sort_by(|left, right| {
            let order = sort_value(left, property).cmp(&sort_value(right, property));
            if desc {
                order.reverse()
            } else {
                order
            }
        });
    }
    rows.into_iter()
        .map(|object| project(object, body))
        .collect()
}

fn filters_match(object: &Object, filters: Option<&Vec<Value>>) -> bool {
    let Some(filters) = filters else {
        return true;
    };
    filters.iter().all(|filter| {
        let key = filter["key"].as_str().unwrap_or("");
        let value = &filter["value"];
        match key {
            "name" => object.name == value.as_str().unwrap_or(""),
            "type" => object.type_key == value.as_str().unwrap_or(""),
            "archived" => object.archived == value.as_bool().unwrap_or(false),
            other => object.properties.get(other) == Some(value),
        }
    })
}

fn sort_value(object: &Object, property: &str) -> String {
    if property == "name" {
        return object.name.clone();
    }
    object.properties[property]
        .as_str()
        .unwrap_or("")
        .to_string()
}

fn project(object: &Object, body: &Value) -> Value {
    let full = object_json(object);
    let Some(fields) = body["fields"].as_array() else {
        return full;
    };
    if fields.is_empty() {
        return full;
    }
    let mut picked = serde_json::Map::new();
    picked.insert("id".into(), full["id"].clone());
    for field in fields {
        let Some(name) = field.as_str() else {
            continue;
        };
        if let Some(value) = full.get(name) {
            picked.insert(name.to_string(), value.clone());
        }
    }
    Value::Object(picked)
}
