//! The two-pass HK → Bots import (docs/anytype-hk-sync-plan.md phase 3).
//! Pass one writes scalar properties, the folded JSON contracts, and the raw
//! remainder; pass two resolves cross-space object links from an in-memory
//! register. HK is never written; dry run is the default and `--apply`
//! performs the writes.

use super::super::client::AnytypeClient;
use super::hk::HK_TYPES;
pub(crate) use super::hk_map::{agent_link_rules, agent_rules, rules, Fmt, Rule, FOLDED};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub(crate) fn natural_key(hk_type: &str, object: &Value) -> String {
    let properties = object["properties"].as_object();
    let first = |keys: &[&str]| {
        keys.iter().find_map(|key| {
            properties
                .and_then(|map| map.get(*key))
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
        })
    };
    let candidates: &[&str] = match hk_type {
        "agent" => &["hermes_profile"],
        "schedule" => &["schedule_key"],
        "subagent" => &["task_key"],
        "skill" => &["skill_key"],
        _ => &["slug", "name"],
    };
    first(candidates)
        .or_else(|| first(&["name"]))
        .map(str::to_string)
        .unwrap_or_else(|| object["id"].as_str().unwrap_or("").to_string())
}

/// HK field value to Bots property value for one rule; `None` is nothing to write.
pub(crate) fn mapped_value(rule: &Rule, value: &Value) -> Option<Value> {
    match rule.fmt {
        Fmt::Bool => value.as_bool().map(Value::Bool),
        Fmt::Json => json_text(value).map(|text| json!(text)),
        Fmt::Text => scalar_text(value).map(|text| json!(scalar_value(&text))),
        Fmt::Link => None,
    }
}

/// Select vocabulary and token normalization; free text is kept verbatim.
pub(crate) fn scalar_value(text: &str) -> String {
    let trimmed = text.trim();
    // A single token (possibly hyphenated) is a select value; prose stays.
    let is_token = !trimmed.contains(' ')
        && trimmed.len() < 40
        && trimmed.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if is_token {
        trimmed.to_ascii_lowercase().replace('-', "_")
    } else {
        trimmed.to_string()
    }
}

/// Value to text: a plain string, or the first of an array, or a named field.
pub(crate) fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Array(items) => items.first().and_then(scalar_text),
        Value::Object(map) => map
            .get("text")
            .or_else(|| map.get("value"))
            .or_else(|| map.get("name"))
            .and_then(scalar_text),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

pub(crate) fn json_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        value if value.as_object().is_some_and(|map| map.is_empty()) => None,
        value if value.as_array().is_some_and(|items| items.is_empty()) => None,
        value => Some(value.to_string()),
    }
}

/// Everything not in the type's rules nor folded: the raw remainder that
/// lands in `m_source_json` so no HK data is dropped.
pub(crate) fn raw_remainder(properties: &Map<String, Value>, hk_type: &str) -> Map<String, Value> {
    let mut mapped: Vec<&str> = rules(hk_type).iter().map(|rule| rule.hk).collect();
    if hk_type == "agent" {
        mapped.extend(agent_rules().iter().map(|rule| rule.hk));
        mapped.extend(agent_link_rules().iter().map(|rule| rule.hk));
        mapped.extend(FOLDED.iter().map(|(hk, _)| *hk));
        mapped.extend(["avatar", "name"]);
    }
    let mut raw = Map::new();
    for (key, value) in properties {
        if !mapped.contains(&key.as_str()) {
            raw.insert(key.clone(), value.clone());
        }
    }
    raw
}

/// The folded JSON contracts for one agent: voice and profile routes.
pub(crate) fn folded_json(properties: &Map<String, Value>) -> Map<String, Value> {
    let mut folded: BTreeMap<&str, Map<String, Value>> = BTreeMap::new();
    for (hk_field, prop) in FOLDED {
        let Some(value) = properties.get(*hk_field) else {
            continue;
        };
        let map = folded.entry(prop).or_default();
        map.insert((*hk_field).to_string(), value.clone());
    }
    folded
        .into_iter()
        .map(|(prop, map)| (prop.to_string(), Value::Object(map)))
        .collect()
}

/// Link ids behind an HK property value: a string, an array, or objects with ids.
pub(crate) fn link_ids(value: &Value) -> Vec<String> {
    match value {
        Value::String(id) if !id.is_empty() => vec![id.clone()],
        Value::Array(items) => items.iter().flat_map(link_ids).collect(),
        Value::Object(map) => match map.get("id") {
            Some(id) => link_ids(id),
            None => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Resolve the link properties of one HK object into Bots object ids.
fn collect_links(
    properties: &Map<String, Value>,
    hk_type: &str,
    register: &BTreeMap<String, String>,
    props: &mut Map<String, Value>,
    unresolved: &mut Vec<String>,
) {
    let mut list = rules(hk_type);
    if hk_type == "agent" {
        list.extend(agent_link_rules());
    }
    for rule in list.iter().filter(|rule| rule.fmt == Fmt::Link) {
        let Some(value) = properties.get(rule.hk) else {
            continue;
        };
        let mut resolved = Vec::new();
        for id in link_ids(value) {
            match register.get(&id) {
                Some(bots_id) => resolved.push(json!(bots_id)),
                None => unresolved.push(format!("{hk_type}.{} -> {id}", rule.hk)),
            }
        }
        if !resolved.is_empty() {
            props.insert(rule.prop.into(), Value::Array(resolved));
        }
    }
}

/// Run the import. `apply` performs the writes; without it the report only
/// counts what would happen. Never writes to the HK space.
pub(crate) async fn run(bots: &AnytypeClient, hk: &AnytypeClient, apply: bool) -> Value {
    let mut register: BTreeMap<String, String> = BTreeMap::new();
    let mut created = Vec::new();
    let mut updated = Vec::new();
    let mut unresolved: Vec<String> = Vec::new();
    let mut raw_fields = 0_i64;
    let mut staged = 0_i64;
    let mut errors: Vec<String> = Vec::new();

    // Existing imported objects per Bots type, keyed by HK source id, so a
    // re-run updates in place instead of duplicating.
    let mut known: BTreeMap<&'static str, BTreeMap<String, String>> = BTreeMap::new();
    for entry in HK_TYPES.iter().filter(|entry| !entry.bots_key.is_empty()) {
        let mut map = BTreeMap::new();
        if let Ok(rows) = bots.search(&json!({ "type": entry.bots_key })).await {
            for row in rows {
                let Some(props) = row["properties"].as_object() else {
                    continue;
                };
                let Some(hk_id) = props.get("m_source_id").and_then(Value::as_str) else {
                    continue;
                };
                if let Some(id) = row["id"].as_str() {
                    map.insert(hk_id.to_string(), id.to_string());
                }
            }
        }
        known.insert(entry.bots_key, map);
    }

    // Pass one: scalar properties, folded contracts, raw remainder.
    for entry in HK_TYPES.iter().filter(|entry| !entry.bots_key.is_empty()) {
        let rows = match hk.search(&json!({ "type": entry.hk_key })).await {
            Ok(rows) => rows,
            Err(error) => {
                errors.push(format!("{}: {}", entry.hk_key, super::error_note(&error)));
                continue;
            }
        };
        for row in &rows {
            let Some(hk_id) = row["id"].as_str().map(str::to_string) else {
                continue;
            };
            let Ok(object) = hk.get_object(&hk_id).await else {
                errors.push(format!("{hk_id}: unreadable"));
                continue;
            };
            let Some(properties) = object["properties"].as_object().cloned() else {
                continue;
            };
            let natural = natural_key(entry.hk_key, &object);
            let mut props = Map::new();
            props.insert("m_source_id".into(), json!(hk_id));
            props.insert("m_source_space".into(), json!("anytype-agents"));
            let raw = raw_remainder(&properties, entry.hk_key);
            raw_fields += raw.len() as i64;
            if !raw.is_empty() {
                props.insert(
                    "m_source_json".into(),
                    json!(serde_json::to_string(&Value::Object(raw)).unwrap_or_default()),
                );
            }
            if entry.hk_key == "agent" {
                for (prop, value) in folded_json(&properties) {
                    props.insert(prop, Value::String(value.to_string()));
                }
            }
            let table = if entry.hk_key == "agent" {
                agent_rules()
            } else {
                rules(entry.hk_key)
            };
            for rule in table.iter().filter(|rule| rule.fmt != Fmt::Link) {
                let Some(value) = properties.get(rule.hk) else {
                    continue;
                };
                if let Some(mapped) = mapped_value(rule, value) {
                    props.insert(rule.prop.into(), mapped);
                }
            }
            staged += 1;
            if apply {
                match upsert(bots, entry.bots_key, &natural, &object, &known, &props).await {
                    Ok((state, id)) => {
                        register.insert(hk_id.clone(), id.clone());
                        let label = format!("{}/{natural}", entry.bots_key);
                        if state == "created" {
                            created.push(label);
                        } else {
                            updated.push(label);
                        }
                    }
                    Err(error) => errors.push(error),
                }
            }
        }
    }

    // Pass two: object links, now that every created id is in the register.
    if apply && !register.is_empty() {
        for entry in HK_TYPES.iter().filter(|entry| !entry.bots_key.is_empty()) {
            let Ok(rows) = hk.search(&json!({ "type": entry.hk_key })).await else {
                continue;
            };
            for row in &rows {
                let Some(hk_id) = row["id"].as_str().map(str::to_string) else {
                    continue;
                };
                let Some(bots_id) = register.get(&hk_id) else {
                    continue;
                };
                let Ok(object) = hk.get_object(&hk_id).await else {
                    continue;
                };
                let Some(properties) = object["properties"].as_object() else {
                    continue;
                };
                let mut props = Map::new();
                collect_links(properties, entry.hk_key, &register, &mut props, &mut unresolved);
                if props.is_empty() {
                    continue;
                }
                let body = json!({
                    "ops": [{ "op": "set_properties", "set": Value::Object(props) }],
                });
                if let Err(error) = bots
                    .patch_object(bots_id, &body, None, &format!("hbots-hk-link:{hk_id}"))
                    .await
                {
                    errors.push(format!("links {hk_id}: {}", super::error_note(&error)));
                }
            }
        }
    }

    json!({
        "apply": apply,
        "created": created,
        "updated": updated,
        "unresolved": unresolved,
        "raw_fields": raw_fields,
        "staged": staged,
        "errors": errors,
    })
}

/// Create or patch one Bots object for an HK object, keyed by `m_source_id`.
async fn upsert(
    bots: &AnytypeClient,
    bots_type: &str,
    natural: &str,
    object: &Value,
    known: &BTreeMap<&'static str, BTreeMap<String, String>>,
    props: &Map<String, Value>,
) -> Result<(String, String), String> {
    let hk_id = props["m_source_id"].as_str().unwrap_or("").to_string();
    let name = object["name"].as_str().unwrap_or(natural).to_string();
    let fingerprint = serde_json::to_string(&Value::Object(props.clone())).unwrap_or_default();
    let idem = format!(
        "hbots-hk:{bots_type}:{natural}:{}",
        crate::storage::adapters::anytype::typed::sha256_hex(fingerprint.as_bytes())
    );
    if let Some(existing) = known.get(bots_type).and_then(|map| map.get(&hk_id)) {
        let body = json!({
            "ops": [{ "op": "set_properties", "set": Value::Object(props.clone()) }],
        });
        bots.patch_object(existing, &body, None, &idem)
            .await
            .map(|_| ("updated".to_string(), existing.clone()))
            .map_err(|error| format!("update {natural}: {}", super::error_note(&error)))
    } else {
        let body = json!({
            "type": bots_type,
            "name": name,
            "properties": Value::Object(props.clone()),
        });
        bots.create_object(&body, &idem)
            .await
            .map(|made| ("created".to_string(), made["id"].as_str().unwrap_or("").to_string()))
            .map_err(|error| format!("create {natural}: {}", super::error_note(&error)))
    }
}
