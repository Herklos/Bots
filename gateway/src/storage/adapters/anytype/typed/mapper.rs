//! Mapping between gateway documents and Anytype typed objects for the P4
//! stores. Shared fields follow the schema catalog; anything not modeled
//! stays machine-local. Property values are read from the nested `properties`
//! a search or GET returns, and written flat, the way the client does.

use serde_json::{json, Map, Value};

pub(crate) const SETTINGS_TYPE: &str = "m_settings";
pub(crate) const BOT_TYPE: &str = "m_bot";
pub(crate) const SETTINGS_NATURAL: &str = "settings";
pub(crate) const MAIN_BOT: &str = "m_main_bot";

/// Bot profile keys the shared object carries. Everything else in `bot.json`
/// is machine-local and lives in the store's complement sidecar.
pub(crate) const BOT_SHARED_KEYS: &[&str] = &[
    "name",
    "shape",
    "color",
    "role",
    "brief",
    "primary",
    "pinned",
    "hidden",
    "hideFromSidebar",
    "notifications",
];

const ANSWER_POLICIES: &[&str] = &["last", "everyone", "chief"];

// ---------- settings ----------

/// The shared subset of a settings document, as remote properties.
pub(crate) fn settings_props(doc: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    put_text(&mut out, "m_user_name", doc.get("userName"));
    if let Some(flag) = doc.get("autoReview").and_then(Value::as_bool) {
        out.insert("m_auto_review".into(), Value::Bool(flag));
    }
    put_text(&mut out, "m_default_runtime_mode", doc.get("defaultRuntimeMode"));
    if let Some(policy) = doc.get("groupAnswerPolicy").and_then(Value::as_str) {
        if ANSWER_POLICIES.contains(&policy) {
            out.insert("m_group_answer_policy".into(), Value::String(policy.into()));
        }
    }
    let rules = doc.get("rules");
    put_json(&mut out, "m_rules_allow_json", rules.and_then(|r| r.get("allow")));
    put_json(&mut out, "m_rules_ask_json", rules.and_then(|r| r.get("ask")));
    put_json(&mut out, "m_sections_json", doc.get("sections"));
    put_json(&mut out, "m_beta_json", doc.get("beta"));
    out
}

/// Overlay remote shared values onto a local settings document. Absent,
/// empty, or malformed remote values leave the machine's value alone.
pub(crate) fn apply_settings_props(doc: &mut Value, props: &Value) {
    let Some(map) = props.as_object() else {
        return;
    };
    let Some(local) = doc.as_object_mut() else {
        return;
    };
    if let Some(text) = text_prop(map, "m_user_name") {
        local.insert("userName".into(), Value::String(text));
    }
    if let Some(flag) = flag_prop(map, "m_auto_review") {
        local.insert("autoReview".into(), Value::Bool(flag));
    }
    if let Some(text) = text_prop(map, "m_default_runtime_mode") {
        local.insert("defaultRuntimeMode".into(), Value::String(text));
    }
    if let Some(policy) = text_prop(map, "m_group_answer_policy") {
        if ANSWER_POLICIES.contains(&policy.as_str()) {
            local.insert("groupAnswerPolicy".into(), Value::String(policy));
        }
    }
    apply_json_prop(local, "rules", "allow", "m_rules_allow_json");
    apply_json_prop(local, "rules", "ask", "m_rules_ask_json");
    apply_json_prop(local, "", "sections", "m_sections_json");
    apply_json_prop(local, "", "beta", "m_beta_json");
}

fn apply_json_prop(local: &mut Map<String, Value>, parent: &str, key: &str, prop: &str) {
    let Some(text) = text_prop(local, prop) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return;
    };
    if parent.is_empty() {
        local.insert(key.into(), value);
    } else if let Some(entry) = local.get_mut(parent).and_then(Value::as_object_mut) {
        entry.insert(key.into(), value);
    } else {
        local.insert(parent.into(), json!({ key: value }));
    }
}

// ---------- bots ----------

/// The shared subset of a Bot profile, as remote properties. `meta` excludes
/// the slug; the store supplies it separately.
pub(crate) fn bot_props(meta: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    if let Some(name) = meta.get("name").and_then(Value::as_str) {
        if !name.trim().is_empty() {
            out.insert("m_label".into(), Value::String(name.to_string()));
        }
    }
    put_text(&mut out, "m_shape", meta.get("shape"));
    put_text(&mut out, "m_color", meta.get("color"));
    if let Some(role) = meta.get("role").and_then(Value::as_str) {
        if role == "bot" || role == "child" {
            out.insert("m_role".into(), Value::String(role.into()));
        }
    }
    put_text(&mut out, "m_brief", meta.get("brief"));
    for (prop, key) in [
        ("m_primary", "primary"),
        ("m_pinned", "pinned"),
        ("m_hidden", "hidden"),
        ("m_hide_from_sidebar", "hideFromSidebar"),
        ("m_notifications", "notifications"),
    ] {
        if let Some(flag) = meta.get(key).and_then(Value::as_bool) {
            out.insert(prop.into(), Value::Bool(flag));
        }
    }
    out
}

/// Rebuild the shared part of a profile from remote properties. Only mapped,
/// present values appear; malformed ones are ignored.
pub(crate) fn bot_meta(props: &Value) -> Map<String, Value> {
    let map = props.as_object();
    let mut meta = Map::new();
    if let Some(name) = map.and_then(|m| text_prop(m, "m_label")) {
        meta.insert("name".into(), Value::String(name));
    }
    for (prop, key) in [("m_shape", "shape"), ("m_color", "color"), ("m_brief", "brief")] {
        if let Some(text) = map.and_then(|m| text_prop(m, prop)) {
            meta.insert(key.into(), Value::String(text));
        }
    }
    if let Some(role) = map.and_then(|m| text_prop(m, "m_role")) {
        if role == "bot" || role == "child" {
            meta.insert("role".into(), Value::String(role));
        }
    }
    for (prop, key) in [
        ("m_primary", "primary"),
        ("m_pinned", "pinned"),
        ("m_hidden", "hidden"),
        ("m_hide_from_sidebar", "hideFromSidebar"),
        ("m_notifications", "notifications"),
    ] {
        if let Some(flag) = map.and_then(|m| flag_prop(m, prop)) {
            meta.insert(key.into(), Value::Bool(flag));
        }
    }
    meta
}

/// The machine-local part of a profile: every key that does not travel.
pub(crate) fn bot_complement(meta: &Value) -> Map<String, Value> {
    meta.as_object()
        .map(|map| {
            map.iter()
                .filter(|(key, _)| {
                    !BOT_SHARED_KEYS.contains(&key.as_str()) && key.as_str() != "slug"
                })
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Read one property out of remote search or GET shapes; both may nest
/// the properties object one level down.
pub(crate) fn properties_at(row: &Value) -> Option<&Map<String, Value>> {
    row.get("properties")
        .or_else(|| row.get("object").and_then(|o| o.get("properties")))
        .and_then(Value::as_object)
}

// ---------- value helpers ----------

fn put_text(out: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
    if let Some(text) = value.and_then(Value::as_str) {
        if !text.trim().is_empty() {
            out.insert(key.into(), Value::String(text.to_string()));
        }
    }
}

fn put_json(out: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
    if let Some(value) = value {
        if value.is_object() || value.is_array() {
            out.insert(key.into(), Value::String(value.to_string()));
        }
    }
}

fn text_prop(map: &Map<String, Value>, key: &str) -> Option<String> {
    let first_text = |value: &Value| value.as_str().map(str::to_string);
    map.get(key)
        .and_then(|value| match value {
            Value::Array(items) => items.first().and_then(first_text),
            value => first_text(value),
        })
        .filter(|text| !text.trim().is_empty())
}

fn flag_prop(map: &Map<String, Value>, key: &str) -> Option<bool> {
    map.get(key).and_then(Value::as_bool)
}
